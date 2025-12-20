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
//! *   **`#[sys_function]`**: Automatically dispatches method calls to platform-specific implementations (e.g., `fn run()` calls `Self::run_impl()`).
//! *   **`#[sys_trait_function]`**: Applies platform configuration to trait method definitions.
//! *   **`#[sys_struct]`**: Generates platform-specific type aliases (e.g., `MyStruct` -> `MyStructLinux`) and optionally enforces trait bounds (e.g., `Send + Sync`) at compile time.
//! *   **`#[platform_mod]`**: Declares platform-dependent modules backed by OS-specific files, with strict visibility control.
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
//! // You then implement the specific logic for each platform:
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
//! This creates handy type aliases for platform-specific builds and allows verifying trait implementations.
//!
//! ```rust
//! # use platify::sys_struct;
//! // 1. Generates `HandleWindows` alias on Windows.
//! // 2. Asserts at compile time that `Handle` implements `Send` and `Sync`.
//! #[sys_struct(traits(Send, Sync), include(windows))]
//! pub struct Handle {
//!     handle: u64,
//! }
//!
//! // Generated code roughly looks like:
//! //
//! // #[cfg(target_os = "windows")]
//! // pub type HandleWindows = Handle;
//! //
//! // #[cfg(target_os = "windows")]
//! // const _: () = {
//! //     fn _assert_traits<T: Send + Sync + ?Sized>() {}
//! //     fn _check() { _assert_traits::<Handle>(); }
//! // };
//! ```
//!
//! ### 3. Using `#[sys_trait_function]`
//!
//! This allows defining methods in a trait that only exist on specific platforms.
//!
//! ```rust
//! # use platify::sys_trait_function;
//! trait DesktopEnv {
//!     /// Only available on Linux
//!     #[sys_trait_function(include(linux))]
//!     fn get_wm_name(&self) -> String;
//! }
//! ```
//!
//! ### 4. Using `#[platform_mod]`
//!
//! This creates module aliases backed by specific files (e.g., `linux.rs`, `windows.rs`).
//!
//! **Note on Visibility:** The actual platform module (e.g., `mod linux;`) inherits the visibility you declare (`pub`), making it accessible to consumers.
//!                         However, the generic alias (`mod driver;`) is generated as a **private** use-statement to be used internally.
//!
//! ```rust,ignore
//! // Assumes existence of `src/linux.rs` and `src/windows.rs`
//!
//! #[platform_mod(include(linux, windows))]
//! pub mod driver;
//!
//! // --- Internal Usage (Platform Agnostic) ---
//! // Inside this file, we use the private alias `driver`.
//! fn init() {
//!     let device = driver::Device::new();
//! }
//! ```
//!
//! **External Consumer Usage:**
//!
//! ```rust,ignore
//! // Users of your crate must explicitly choose the platform module.
//! // 'driver' is not visible here.
//! #[cfg(target_os = "linux")]
//! use my_crate::linux::Device;
//! ```

use proc_macro::TokenStream;
use proc_macro2::{Span as Span2, TokenStream as TokenStream2};
use quote::{format_ident, quote, ToTokens as _};
use std::collections::{BTreeSet, HashSet};
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned as _;
use syn::{
    parenthesized, parse, parse_macro_input, token, Attribute, ConstParam, Error, FnArg,
    ForeignItemFn, GenericParam, ItemFn, ItemMod, ItemStruct, ItemUse, Pat, PatType, ReturnType,
    Signature, TraitItemFn, TypeParam, UseTree, Visibility,
};

/// Applies platform configuration to trait method definitions.
///
/// Use this inside a `trait` definition to limit methods to specific platforms.
///
/// # Options
///
/// - `include(...)`: Whitelist of platforms. Options: `linux`, `macos`, `windows`, `all`, `posix`.
/// - `exclude(...)`: Blacklist of platforms. Removes them from the included set.
#[proc_macro_attribute]
pub fn sys_trait_function(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr = parse_macro_input!(attr as AttrOptions);
    let cfg_attr = attr.convert_to_cfg_attr();

    let trait_fn = parse_macro_input!(item as TraitItemFn);

    quote! {
        #cfg_attr
        #trait_fn
    }
    .into()
}

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

    let struct_info = match parse::<ForeignItemFn>(item.clone()) {
        Ok(foreign_item_fn) => foreign_item_fn,
        Err(_) => {
            return match parse::<ItemFn>(item) {
                Ok(item_fn) => {
                    quote! {
                        #cfg_attr
                        #item_fn
                    }
                }
                Err(err) => err.to_compile_error(),
            }
            .into()
        }
    };

    let ForeignItemFn {
        attrs,
        vis,
        sig,
        semi_token: _,
    } = struct_info;

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
        #cfg_attr
        #(#attrs)*
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
/// - `traits(...)`: Comma-separated list of traits (e.g., `Send, Sync`) to assert at compile time.
/// - `include(...)`: Whitelist of platforms.
/// - `exclude(...)`: Blacklist of platforms.
///
/// (See [`sys_function`] for more details on include/exclude logic).
#[proc_macro_attribute]
pub fn sys_struct(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr = parse_macro_input!(attr as StructOptions);
    let cfg_attr = attr.options.convert_to_cfg_attr();

    let item_struct = parse_macro_input!(item as ItemStruct);
    let &ItemStruct {
        attrs: _,
        vis: _,
        struct_token: _,
        ref ident,
        ref generics,
        fields: _,
        semi_token: _,
    } = &item_struct;

    let trait_asserts = if attr.traits.is_empty() {
        TokenStream2::new()
    } else {
        let traits = attr.traits;
        let generics_where_clause = generics.where_clause.as_ref();

        let generics_without_lifetime = generics
            .params
            .iter()
            .filter_map(|generic_param| match *generic_param {
                GenericParam::Lifetime(_) => None,
                GenericParam::Type(ref type_param) => {
                    let &TypeParam {
                        ref attrs,
                        ref ident,
                        ref colon_token,
                        ref bounds,
                        eq_token: _,
                        default: _,
                    } = type_param;
                    Some(quote!(#(#attrs)* #ident #colon_token #bounds))
                }
                GenericParam::Const(ref const_param) => {
                    let &ConstParam {
                        ref attrs,
                        ref const_token,
                        ref ident,
                        ref colon_token,
                        ref ty,
                        eq_token: _,
                        default: _,
                    } = const_param;
                    Some(quote!(#(#attrs)* #const_token #ident #colon_token #ty))
                }
            })
            .collect::<Vec<_>>();
        let generics_without_lifetime = if generics_without_lifetime.is_empty() {
            TokenStream2::new()
        } else {
            quote!(<#(#generics_without_lifetime),*>)
        };

        let generics_usages = if generics.params.is_empty() {
            TokenStream2::new()
        } else {
            let generics_usages =
                generics
                    .params
                    .iter()
                    .map(|generic_param| match *generic_param {
                        GenericParam::Lifetime(_) => quote!('_),
                        GenericParam::Type(ref type_param) => type_param.ident.to_token_stream(),
                        GenericParam::Const(ref const_param) => const_param.ident.to_token_stream(),
                    });
            quote!(<#(#generics_usages),*>)
        };

        quote! {
            #cfg_attr
            const _: () = {
                fn _assert_traits<T: #(#traits)+* + ?Sized>() {}
                fn _check #generics_without_lifetime() #generics_where_clause { _assert_traits::<#ident #generics_usages>(); }
            };
        }
    };

    quote! {
        #cfg_attr
        #item_struct
        #trait_asserts
    }
    .into()
}

/// Declares a platform-dependent module backed by OS-specific source files.
///
/// This attribute simplifies the management of platform-specific code modules. Instead of manually
/// writing multiple `#[cfg(...)] mod ...;` blocks, you define a single logical module name.
/// The macro expects corresponding files (e.g., `linux.rs`, `windows.rs`) to exist in the same directory.
///
/// # Options
///
/// Same as [`sys_function`]: `include(...)` and `exclude(...)` determine which platform modules are generated.
///
/// # Visibility Behavior
///
/// This macro enforces a strict separation between **internal convenience** and **external access**:
///
/// 1. **The Module (External):** The actual platform module (e.g., `mod linux;`) **inherits** the visibility you declared.
///    If you write `pub mod driver;`, the generated `mod linux;` will be public.
/// 2. **The Alias (Internal):** The logical name you specified (e.g., `driver`) is generated as a **private use-alias**.
///
/// **Why?** This ensures that external consumers of your crate must be explicit about the platform they are accessing
/// (e.g., `my_crate::linux::MyStruct`), while allowing you to use the generic name (e.g., `driver::MyStruct`)
/// conveniently within your own code.
#[proc_macro_attribute]
pub fn platform_mod(attr: TokenStream, item: TokenStream) -> TokenStream {
    struct DModInfo {
        attrs: Vec<Attribute>,
        vis: Visibility,
        ident: proc_macro2::Ident,
    }

    let attr = parse_macro_input!(attr as AttrOptions);
    let allowed_set: BTreeSet<_> = attr.allowed_set(|platform| match platform {
        Platform::All | Platform::Posix => unreachable!("Should have been expanded"),
        Platform::Linux => "linux",
        Platform::Macos => "macos",
        Platform::Windows => "windows",
    });

    let mod_info = match parse::<ItemUse>(item.clone()) {
        Ok(item_use) => {
            let ItemUse {
                attrs,
                vis,
                use_token: _,
                leading_colon,
                tree,
                semi_token: _,
            } = item_use;

            if let Some(leading_colon) = leading_colon {
                return Error::new(
				    leading_colon.span(),
				    "#[platform_mod] does not support absolute paths (leading `::`). Please use a local identifier"
			    ).to_compile_error().into();
            }

            let use_ident = match tree {
                UseTree::Name(use_name) => use_name.ident,
                other @ (UseTree::Path(_)
                | UseTree::Rename(_)
                | UseTree::Glob(_)
                | UseTree::Group(_)) => {
                    return Error::new(
					    other.span(),
					    "#[platform_mod] on `use` statements only supports simple direct aliases (e.g., `use name;`)"
				    ).to_compile_error().into();
                }
            };

            DModInfo {
                attrs,
                vis,
                ident: use_ident,
            }
        }
        Err(_) => match parse::<ItemMod>(item) {
            Ok(item_mod) => {
                let item_mod_span = item_mod.span();

                let ItemMod {
                    attrs,
                    vis,
                    unsafety,
                    mod_token: _,
                    ident,
                    content,
                    semi: _,
                } = item_mod;

                if let Some(unsafety) = unsafety {
                    return Error::new(
                        unsafety.span(),
                        "#[platform_mod] does not support `unsafe` modules",
                    )
                    .to_compile_error()
                    .into();
                }

                if content.is_some() {
                    return Error::new(
					    item_mod_span,
					    "#[platform_mod] does not support inline modules with a body `{ ... }`.\n\
					    Please use a declaration like `mod name;` to allow swapping the file based on the platform."
				    ).to_compile_error().into();
                }

                DModInfo { attrs, vis, ident }
            }
            Err(_) => {
                return Error::new(
				    Span2::call_site(),
				    "#[platform_mod] expected a `mod declaration` (e.g., `mod foo;`) or a `use statement` (e.g., `use foo;`)"
			    ).to_compile_error().into();
            }
        },
    };

    let DModInfo { attrs, vis, ident } = mod_info;

    let mods = allowed_set.into_iter().map(|platform| {
        let platform_ident = format_ident!("{platform}");

        quote! {
            #[cfg(target_os = #platform)]
            #(#attrs)*
            #vis mod #platform_ident;
            #[cfg(target_os = #platform)]
            #(#attrs)*
            use #platform_ident as #ident;
        }
    });

    quote!(#(#mods)*).into()
}

// ##################################### IMPLEMENTATION #####################################

mod keywords {
    use syn::custom_keyword;

    custom_keyword!(traits);

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
    span: Span2,
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
        parse_attributes(input, false).map(|options| {
            let StructOptions { options, traits } = options;
            assert_eq!(traits.len(), 0, "Implementation error");
            options
        })
    }
}

struct StructOptions {
    options: AttrOptions,
    traits: Vec<syn::Path>,
}

impl Parse for StructOptions {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        parse_attributes(input, true)
    }
}

fn parse_attributes(input: ParseStream<'_>, allow_traits: bool) -> syn::Result<StructOptions> {
    let mut result = StructOptions {
        options: AttrOptions {
            span: input.span(),
            exclude: HashSet::default(),
            include: HashSet::default(),
        },
        traits: Vec::default(),
    };

    while !input.is_empty() {
        let lookahead = input.lookahead1();

        if allow_traits && lookahead.peek(keywords::traits) {
            input.parse::<keywords::traits>()?;

            let content;
            parenthesized!(content in input);

            let traits = content.parse_terminated(syn::Path::parse, token::Comma)?;
            result.traits.extend(traits);
        } else if lookahead.peek(keywords::exclude) {
            input.parse::<keywords::exclude>()?;

            let content;
            parenthesized!(content in input);

            let platforms = content.parse_terminated(Platform::parse, token::Comma)?;
            result.options.exclude.extend(platforms);
        } else if lookahead.peek(keywords::include) {
            input.parse::<keywords::include>()?;

            let content;
            parenthesized!(content in input);

            let platforms = content.parse_terminated(Platform::parse, token::Comma)?;
            result.options.include.extend(platforms);
        } else {
            return Err(lookahead.error());
        }

        if !input.is_empty() {
            input.parse::<token::Comma>()?;
        }
    }

    if result.options.include.is_empty() {
        result.options.include.insert(Platform::All);
    }

    Ok(result)
}
