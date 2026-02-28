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
//! *   **`#[sys_function]`**: Generates a wrapper function that dispatches calls to a platform-specific implementation (e.g., `fn run()` calls `Self::run_impl()`).
//! *   **`#[sys_trait_function]`**: Simple wrapper to apply platform-specific `#[cfg(...)]` to trait methods.
//! *   **`#[sys_struct]` / `#[sys_enum]`**: Applies platform gating to types and optionally enforces trait bounds (e.g., `Send + Sync`) at compile time.
//! *   **`#[platform_mod]`**: Declares platform-dependent modules backed by OS-specific files, providing a unified internal alias.
//!
//! ## Supported Keywords
//!
//! Inside `include(...)` and `exclude(...)`, you can use:
//!
//! *   **Platforms**: `linux`, `macos`, `windows`
//! *   **Groups**: `posix` (Linux + macOS), `all` (Linux, macOS, Windows)
//!
//! ## Logic
//!
//! 1. Start with the `include` list (defaults to `all` if omitted).
//! 2. Remove any platforms specified in the `exclude` list.
//! 3. Generate the resulting `#[cfg(any(target_os = "..."))]` attribute.
//!
//! ---
//!
//! ## Examples
//!
//! ### 1. Using `#[sys_function]`
//!
//! This macro generates a method body that delegates to an implementation suffixed with `_impl`.
//!
//! ```rust
//! # use platify::sys_function;
//! struct SystemManager;
//!
//! impl SystemManager {
//!     /// Dispatched on all platforms. Calls `Self::reboot_impl`.
//!     #[sys_function]
//!     pub fn reboot(&self) -> Result<(), String>;
//!
//!     /// Only available on Linux. Calls `Self::update_kernel_impl`.
//!     #[sys_function(include(linux))]
//!     pub fn update_kernel(&self);
//! }
//!
//! impl SystemManager {
//!     #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
//!     fn reboot_impl(&self) -> Result<(), String> { Ok(()) }
//!
//!     #[cfg(target_os = "linux")]
//!     fn update_kernel_impl(&self) { println!("Updating..."); }
//! }
//! ```
//!
//! ### 2. Using `#[sys_struct]` and `traits`
//!
//! Unlike standard `#[cfg]`, this allows you to verify that your platform-specific type actually implements
//! specific traits, preventing "missing implementation" errors at a later stage.
//!
//! ```rust
//! # use platify::sys_struct;
//! // This struct only exists on Windows and MUST implement Send and Sync.
//! #[sys_struct(traits(Send, Sync), include(windows))]
//! pub struct WinHandle {
//!     handle: u64,
//! }
//! ```
//!
//! ### 3. Using `#[platform_mod]`
//!
//! This automates the pattern of having a `linux.rs` and `windows.rs` and aliasing them to a common name.
//!
//! ```rust,ignore
//! // In src/lib.rs
//! // 1. Generates: #[cfg(target_os = "linux")] pub mod linux;
//! // 2. Generates: #[cfg(target_os = "linux")] use linux as driver;
//! #[platform_mod(include(linux, windows))]
//! pub mod driver;
//!
//! fn init() {
//!     // Use the private alias 'driver' internally regardless of the OS.
//!     driver::init_hardware();
//! }
//! ```

use proc_macro::TokenStream;
use proc_macro2::{Ident as Ident2, Span as Span2, TokenStream as TokenStream2};
use quote::{format_ident, quote, ToTokens as _};
use std::borrow::Cow;
use std::collections::{BTreeSet, HashSet};
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned as _;
use syn::{
    parenthesized, parse, parse_macro_input, token, Attribute, ConstParam, Error, FnArg,
    ForeignItemFn, GenericParam, Generics, ItemEnum, ItemFn, ItemMod, ItemStruct, ItemUse, Pat,
    PatType, ReturnType, Signature, TraitItemFn, TypeParam, UseTree, Visibility,
};

/// Applies platform configuration to trait method definitions.
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

/// Generates a platform-dependent method implementation dispatcher.
///
/// It transforms a signature like `fn foo(&self)` into a body `{ Self::foo_impl(self) }`.
/// It supports `async`, `unsafe`, and generic parameters.
///
/// **Note:** Complex patterns in arguments (e.g., `fn move_point((x, y): (i32, i32))`) are not
/// supported. Use simple identifiers for arguments.
#[proc_macro_attribute]
pub fn sys_function(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr = parse_macro_input!(attr as AttrOptions);
    let cfg_attr = attr.convert_to_cfg_attr();

    let Ok(struct_info) = parse::<ForeignItemFn>(item.clone()) else {
        return match parse::<ItemFn>(item) {
            Ok(item_fn) => {
                quote! {
                    #cfg_attr
                    #item_fn
                }
            }
            Err(err) => err.to_compile_error(),
        }
        .into();
    };

    let ForeignItemFn {
        attrs,
        vis,
        sig,
        semi_token: _,
    } = struct_info;

    let Signature {
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
    } = sig;

    let has_deprecated = attrs.iter().any(|attr| attr.path().is_ident("deprecated"));

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

    let expect_deprecated = if has_deprecated {
        quote!(#[expect(deprecated, reason = "Deprecated due to code generation constraints")])
    } else {
        TokenStream2::new()
    };

    let result = quote! {
        #cfg_attr
        #(#attrs)*
        #vis #sig {
            #expect_deprecated
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

/// Applies platform config to an enum and verifies trait bounds at compile time.
///
/// Use `traits(Trait1, Trait2)` to ensure the type satisfies these bounds on the target platform.
#[proc_macro_attribute]
pub fn sys_enum(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr = parse_macro_input!(attr as ParsedAttrOptions);
    let cfg_attr = attr.options.convert_to_cfg_attr();

    let item_enum = parse_macro_input!(item as ItemEnum);
    let ItemEnum {
        attrs: _,
        vis: _,
        enum_token: _,
        ref ident,
        ref generics,
        brace_token: _,
        variants: _,
    } = item_enum;

    let trait_asserts = generate_assert_check(&attr, ident, generics, Some(&cfg_attr));

    quote! {
        #cfg_attr
        #item_enum
        #trait_asserts
    }
    .into()
}

/// Applies platform config to a struct and verifies trait bounds at compile time.
///
/// Use `traits(Trait1, Trait2)` to ensure the type satisfies these bounds on the target platform.
#[proc_macro_attribute]
pub fn sys_struct(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr = parse_macro_input!(attr as ParsedAttrOptions);
    let cfg_attr = attr.options.convert_to_cfg_attr();

    let item_struct = parse_macro_input!(item as ItemStruct);
    let ItemStruct {
        attrs: _,
        vis: _,
        struct_token: _,
        ref ident,
        ref generics,
        fields: _,
        semi_token: _,
    } = item_struct;

    let trait_asserts = generate_assert_check(&attr, ident, generics, Some(&cfg_attr));

    quote! {
        #cfg_attr
        #item_struct
        #trait_asserts
    }
    .into()
}

/// Declares platform-dependent modules with a unified internal alias.
///
/// For each platform, it generates:
/// 1. A module declaration (e.g., `pub mod linux;`) using the platform name.
/// 2. A private `use` alias (e.g., `use linux as my_mod;`) using the identifier you provided.
///
/// This allows external users to see the platform-specific modules, while your internal
/// code uses the generic alias.
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

#[must_use]
fn generate_assert_check(
    options: &ParsedAttrOptions,
    ident: &Ident2,
    generics: &Generics,
    cfg_attr: Option<&TokenStream2>,
) -> TokenStream2 {
    if options.traits.is_empty() {
        return TokenStream2::new();
    }

    let cfg_attr = cfg_attr.map_or_else(
        || Cow::Owned(options.options.convert_to_cfg_attr()),
        Cow::Borrowed,
    );

    let traits = &options.traits;
    let generics_where_clause = generics.where_clause.as_ref();

    let generics_without_lifetime = generics
        .params
        .iter()
        .filter_map(|generic_param| match *generic_param {
            GenericParam::Lifetime(_) => None,
            GenericParam::Type(ref type_param) => {
                let TypeParam {
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
                let ConstParam {
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
        let generics_usages = generics
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
            fn assert_traits<T: #(#traits)+* + ?Sized>() {}
            fn _check #generics_without_lifetime() #generics_where_clause { assert_traits::<#ident #generics_usages>(); }
        };
    }
}

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
            let ParsedAttrOptions { options, traits } = options;
            assert_eq!(traits.len(), 0, "Implementation error");
            options
        })
    }
}

struct ParsedAttrOptions {
    options: AttrOptions,
    traits: Vec<syn::Path>,
}

impl Parse for ParsedAttrOptions {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        parse_attributes(input, true)
    }
}

fn parse_attributes(input: ParseStream<'_>, allow_traits: bool) -> syn::Result<ParsedAttrOptions> {
    let mut result = ParsedAttrOptions {
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
