//! # Platify
//!
//! **Platify** streamlines the development of cross-platform Rust applications by reducing the boilerplate
//! associated with `#[cfg(...)]` attributes.
//!
//! Instead of manually cluttering your code with complex `cfg` checks and duplicate function definitions,
//! Platify allows you to define platform-specific behavior using a clean, declarative attribute syntax.
//!
//! ## Features
//!
//! *   **`#[sys_function]`**: Automatically dispatches method calls to platform-specific implementations based on the OS.
//! *   **`#[sys_struct]`**: Generates platform-specific type aliases for structs (e.g., `MyStructLinux`, `MyStructWindows`).
//! *   **Flexible Logic**: Supports explicit inclusion (`include`) and exclusion (`exclude`) of platforms.
//! *   **Platform Groups**: Includes helper keywords like `posix` (Linux + macOS) or `all`.
//!
//! ## Supported Keywords
//!
//! The following keywords can be used inside `include(...)` and `exclude(...)`:
//!
//! *   `linux`
//! *   `macos`
//! *   `windows`
//! *   `posix` (Expands to: `linux`, `macos`)
//! *   `all` (Expands to: `linux`, `macos`, `windows`)
//!
//! ## Logic
//!
//! The set of allowed platforms is calculated as follows:
//! 1. Start with the `include` list. If `include` is omitted, it defaults to `all`.
//! 2. Remove any platforms specified in the `exclude` list.
//! 3. Generate the corresponding `#[cfg(any(...))]` attributes.
//!
//! ---
//!
//! ## Examples
//!
//! ### 1. Using `#[sys_function]`
//!
//! This macro generates a default method that delegates to a `_impl` suffixed method.
//!
//! ```rust
//! # use platify::sys_function;
//! struct SystemManager;
//!
//! impl SystemManager {
//!     /// This method is available on ALL supported platforms (default).
//!     /// It calls `reboot_impl` internally.
//!     #[sys_function]
//!     pub fn reboot(&self) -> Result<(), String>;
//!
//!     /// This method is ONLY available on Linux.
//!     #[sys_function(include(linux))]
//!     pub fn update_kernel(&self);
//!
//!     /// This method is available on Linux and macOS, but NOT Windows.
//!     #[sys_function(exclude(windows))]
//!     pub fn posix_magic(&self);
//! }
//!
//! // You then implement the specific logic:
//! impl SystemManager {
//!     #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
//!     fn reboot_impl(&self) -> Result<(), String> {
//!         Ok(())
//!     }
//!
//!     #[cfg(target_os = "linux")]
//!     fn update_kernel_impl(&self) {
//!         println!("Updating Linux kernel...");
//!     }
//!
//!     #[cfg(any(target_os = "linux", target_os = "macos"))]
//!     fn posix_magic_impl(&self) {
//!         println!("Running POSIX specific logic");
//!     }
//! }
//! ```
//!
//! ### 2. Using `#[sys_struct]`
//!
//! This creates handy type aliases for platform-specific builds.
//!
//! ```rust
//! # use platify::sys_struct;
//! #[sys_struct(include(windows))]
//! pub struct Handle {
//!     handle: u64,
//! }
//!
//! // Generates:
//! // #[cfg(target_os = "windows")]
//! // pub type HandleWindows = Handle;
//! ```

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{format_ident, quote, ToTokens as _};
use std::collections::{BTreeSet, HashSet};
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned as _;
use syn::{
	parenthesized, parse_macro_input, token, Error, FnArg, ForeignItemFn, GenericParam, ItemStruct,
	Pat, PatType, ReturnType, Signature,
};

/// Generates a platform-dependent method implementation.
///
/// This attribute macro acts as a dispatcher. It applies `#[cfg(...)]` attributes based on the
/// provided configuration and generates a default body that calls a platform-specific implementation
/// (e.g., `fn foo()` calls `Self::foo_impl()`).
///
/// # Options
///
/// - `include(...)`: Whitelist of platforms. Options: `linux`, `macos`, `windows`, `all`, `posix`.
/// - `exclude(...)`: Blacklist of platforms. Removes them from the included set.
///
/// If `include` is omitted, it defaults to `all` (minus any exclusions).
///
/// # Logic
///
/// 1. Calculates the set of allowed platforms: `(include OR all) - exclude`.
/// 2. Applies `#[cfg(any(target_os = "..."))]` to the method.
/// 3. Generates a default implementation: `fn foo(&self) { Self::foo_impl(self) }`.
///
/// # Requirements
///
/// The implementing type must define the corresponding `_impl` method.
#[proc_macro_attribute]
pub fn sys_function(attr: TokenStream, item: TokenStream) -> TokenStream {
	let attr = parse_macro_input!(attr as AttrOptions);
	let cfg_attr = attr.convert_to_cfg_attr();

	let ForeignItemFn {
		attrs,
		vis,
		sig,
		semi_token: _,
	} = parse_macro_input!(item);

	let &Signature {
		constness: _,
		ref asyncness,
		ref unsafety,
		abi: _,
		fn_token: _,
		ref ident,
		ref generics,
		paren_token: _,
		ref inputs,
		ref variadic,
		ref output,
	} = &sig;

	let sys_ident = format_ident!("{ident}_impl");
	let asyncness = asyncness
		.as_ref()
		.map_or_else(TokenStream2::new, |_| quote!(.await));
	let output_semicolon = if matches!(output, ReturnType::Default) {
		quote!(;)
	} else {
		TokenStream2::new()
	};

	let mut param_errors = TokenStream2::new();
	let input_names = inputs.iter().filter_map(|fn_arg| match *fn_arg {
		FnArg::Receiver(_) => Some(quote!(self)),
		FnArg::Typed(PatType { ref pat, .. }) => match **pat {
			Pat::Ident(ref pat_ident) => Some(pat_ident.ident.to_token_stream()),
			ref other => {
				const MSG: &str = "Complex patterns in arguments are not supported by #[sys_function]: give the argument a name";
				param_errors.extend(Error::new(other.span(), MSG).to_compile_error());
				None
			},
		},
	});

	let generic_names = generics
		.params
		.iter()
		.filter_map(|generic_param| match *generic_param {
			GenericParam::Lifetime(_) => None,
			GenericParam::Type(ref type_param) => Some(type_param.ident.to_token_stream()),
			GenericParam::Const(ref const_param) => Some(const_param.ident.to_token_stream()),
		})
		.collect::<Vec<_>>();
	let generic_names = if generic_names.is_empty() {
		TokenStream2::new()
	} else {
		quote!(::<#(#generic_names),*>)
	};

	let mut body = quote! {
		Self::#sys_ident #generic_names(#(#input_names),*)#asyncness #output_semicolon
	};
	if unsafety.is_some() {
		body = quote!(unsafe { #body });
	}

	let result = quote! {
		#(#attrs)*
		#cfg_attr
		#vis #sig {
			#body
		}
	};

	let variadic_error = variadic
		.as_ref()
		.map_or_else(TokenStream2::new, |variadic| {
			Error::new(variadic.dots.span(), "Variadic arguments are not permitted")
				.to_compile_error()
		});

	quote! {
		#result
		#param_errors
		#variadic_error
	}
	.into()
}

/// Generates platform-specific type aliases for a struct.
///
/// It preserves the original struct definition and adds type aliases that are only available
/// on specific platforms.
///
/// # Options
///
/// Same as [`sys_function`].
#[proc_macro_attribute]
pub fn sys_struct(attr: TokenStream, item: TokenStream) -> TokenStream {
	let attr = parse_macro_input!(attr as AttrOptions);
	let allowed_set: BTreeSet<_> = attr.allowed_set(|platform| match platform {
		Platform::All | Platform::Posix => unreachable!("Should have been expanded"),
		Platform::Linux => ("linux", "Linux"),
		Platform::Macos => ("macos", "MacOS"),
		Platform::Windows => ("windows", "Windows"),
	});

	let item_struct = parse_macro_input!(item as ItemStruct);
	let &ItemStruct {
		ref attrs,
		ref vis,
		struct_token: _,
		ref ident,
		ref generics,
		fields: _,
		semi_token: _,
	} = &item_struct;

	let deprecated_attr = attrs
		.iter()
		.find(|next_attr| next_attr.path().is_ident("deprecated"));

	let generics_names = if generics.params.is_empty() {
		TokenStream2::new()
	} else {
		let generics_names = generics
			.params
			.iter()
			.map(|generic_param| match *generic_param {
				GenericParam::Lifetime(ref lifetime_param) => {
					lifetime_param.lifetime.to_token_stream()
				}
				GenericParam::Type(ref type_param) => type_param.ident.to_token_stream(),
				GenericParam::Const(ref const_param) => const_param.ident.to_token_stream(),
			});
		quote!(<#(#generics_names),*>)
	};

	let aliases = allowed_set.into_iter().map(|(platform, ident_postfix)| {
		let deprecated_attr = deprecated_attr.map_or_else(
			TokenStream2::new,
			|deprecated_attr| quote!(#deprecated_attr),
		);
		let doc_msg = format!("Platform-specific alias for [{ident}].");
		let alias_ident = format_ident!("{ident}{ident_postfix}");
		quote! {
			#[doc = #doc_msg]
			#deprecated_attr
			#[cfg(target_os = #platform)]
			#vis type #alias_ident #generics = #ident #generics_names;
		}
	});

	quote! {
		#(#aliases)*
		#item_struct
	}
	.into()
}

// ##################################### IMPLEMENTATION #####################################

mod keywords {
	use syn::custom_keyword;

	custom_keyword!(exclude);
	custom_keyword!(include);

	custom_keyword!(all);
	custom_keyword!(posix);
	custom_keyword!(linux);
	custom_keyword!(macos);
	custom_keyword!(windows);
}

#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum Platform {
	All,
	Posix,
	Linux,
	Macos,
	Windows,
}

impl Platform {
	#[must_use]
	fn expand(self) -> Vec<Self> {
		match self {
			Self::All => vec![Self::Linux, Self::Macos, Self::Windows],
			Self::Posix => vec![Self::Linux, Self::Macos],
			Self::Linux | Self::Macos | Self::Windows => vec![self],
		}
	}
}

impl Parse for Platform {
	fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
		let lookahead = input.lookahead1();
		if lookahead.peek(keywords::all) {
			input.parse::<keywords::all>()?;
			Ok(Self::All)
		} else if lookahead.peek(keywords::posix) {
			input.parse::<keywords::posix>()?;
			Ok(Self::Posix)
		} else if lookahead.peek(keywords::linux) {
			input.parse::<keywords::linux>()?;
			Ok(Self::Linux)
		} else if lookahead.peek(keywords::macos) {
			input.parse::<keywords::macos>()?;
			Ok(Self::Macos)
		} else if lookahead.peek(keywords::windows) {
			input.parse::<keywords::windows>()?;
			Ok(Self::Windows)
		} else {
			Err(lookahead.error())
		}
	}
}

struct AttrOptions {
	span: Span,
	exclude: HashSet<Platform>,
	include: HashSet<Platform>,
}

impl AttrOptions {
	#[must_use]
	fn allowed_set<B: FromIterator<O>, M: Fn(Platform) -> O, O>(&self, mapping: M) -> B {
		let all_includes = self
			.include
			.iter()
			.copied()
			.flat_map(Platform::expand)
			.collect::<HashSet<_>>();
		let all_excludes = self
			.exclude
			.iter()
			.copied()
			.flat_map(Platform::expand)
			.collect::<HashSet<_>>();
		all_includes
			.difference(&all_excludes)
			.map(|platform| mapping(*platform))
			.collect()
	}

	#[must_use]
	fn convert_to_cfg_attr(&self) -> TokenStream2 {
		let allowed_set: BTreeSet<_> = self.allowed_set(|platform| match platform {
			Platform::All | Platform::Posix => unreachable!("Should have been expanded"),
			Platform::Linux => "linux",
			Platform::Macos => "macos",
			Platform::Windows => "windows",
		});

		let error = if allowed_set.is_empty() {
			Error::new(
				self.span,
				"Configuration excludes all platforms: 'include' and 'exclude' cancel each other out",
			)
			.to_compile_error()
		} else {
			TokenStream2::new()
		};

		let mut cfg_attrs = quote!(#(target_os = #allowed_set),*);
		if allowed_set.len() != 1 {
			cfg_attrs = quote!(any(#cfg_attrs));
		}

		quote! {
			#error
			#[cfg(#cfg_attrs)]
		}
	}
}

impl Parse for AttrOptions {
	fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
		let mut result = Self {
			span: input.span(),
			exclude: HashSet::default(),
			include: HashSet::default(),
		};

		while !input.is_empty() {
			let lookahead = input.lookahead1();

			if lookahead.peek(keywords::exclude) {
				input.parse::<keywords::exclude>()?;

				let content;
				parenthesized!(content in input);

				let platforms = content.parse_terminated(Platform::parse, token::Comma)?;
				result.exclude.extend(platforms);
			} else if lookahead.peek(keywords::include) {
				input.parse::<keywords::include>()?;

				let content;
				parenthesized!(content in input);

				let platforms = content.parse_terminated(Platform::parse, token::Comma)?;
				result.include.extend(platforms);
			} else {
				return Err(lookahead.error());
			}

			if !input.is_empty() {
				input.parse::<token::Comma>()?;
			}
		}

		if result.include.is_empty() {
			result.include.insert(Platform::All);
		}

		Ok(result)
	}
}
