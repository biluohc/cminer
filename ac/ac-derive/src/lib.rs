#![recursion_limit = "128"]
#[macro_use]
extern crate darling;
extern crate proc_macro;

use crate::proc_macro::TokenStream;
use proc_macro2::{Ident, Span, TokenStream as TokenStream2};

use darling::FromField;
use heck::{ShoutySnakeCase, SnakeCase};
use quote::{quote, ToTokens};
use syn::{Data, Field};

use std::collections::BTreeSet as Set;
use std::convert::TryFrom;
use std::fmt::Debug;

#[proc_macro_derive(Ac, attributes(ac))]
pub fn derive_ac(input: TokenStream) -> TokenStream {
    let ast = syn::parse(input).unwrap();
    impl_ac_macro(&ast)
}

#[derive(Default, FromVariant, FromField)]
#[darling(attributes(ac), default)]
struct AcAttrs {
    skip: bool,
    default: Option<usize>,
}

fn gen_ts<N>(ty: &str, default: usize, ident: &Ident, name: Ident) -> Option<(TokenStream2, TokenStream2, Ident)>
where
    N: ToTokens + TryFrom<usize> + Copy,
    <N as TryFrom<usize>>::Error: Debug,
{
    let default = N::try_from(default).map_err(|e| panic!(format!("{}::try_from(default: {}) failed: {:?}", ident, default, e))).unwrap();
    let ty = Ident::new(ty, Span::call_site());

    let define = quote! {
    ///pub static #name: #ty = #ty::new(#default);
    pub static #name: #ty = #ty::new(#default);
    };
    let impl_ = quote! {
        pub fn #ident() -> &'static #ty {
            &#name
        }
    };

    Some((define, impl_, ty))
}

fn field_to_ts(field: &Field) -> Option<(TokenStream2, TokenStream2, Ident)> {
    let ty = field.ty.to_token_stream();
    let tystr = ty.to_string();

    let attrs = AcAttrs::from_field(field);
    match (field.ident.as_ref(), attrs) {
        (Some(ident), Ok(ac)) if !ac.skip => {
            let default = ac.default.unwrap_or(0);
            let name = Ident::new(&ident.to_string().to_shouty_snake_case(), Span::call_site());

            match tystr.as_str() {
                "usize" => gen_ts::<usize>("AcUsize", default, ident, name),
                "u64" => gen_ts::<u64>("AcU64", default, ident, name),
                "u32" => gen_ts::<u32>("AcU32", default, ident, name),
                "u16" => gen_ts::<u16>("AcU16", default, ident, name),
                "u8" => gen_ts::<u8>("AcU8", default, ident, name),
                unkown => panic!(format!("unsupported field type: {}", unkown)),
            }
        }
        (ident, Err(ace)) => panic!(format!("parse Attrs::from_field({:?}) failed: {:?}", ident, ace)),
        (None, _) => panic!("unsupported field unamed"),
        _ => None,
    }
}

fn impl_ac_macro(ast: &syn::DeriveInput) -> TokenStream {
    let mut defines = TokenStream2::new();
    let mut impls = TokenStream2::new();

    let mut tys = Set::new();
    match &ast.data {
        Data::Struct(data) => {
            for (_idx, field) in data.fields.iter().enumerate() {
                if let Some((define, impl_, using)) = field_to_ts(field) {
                    defines.extend(define);
                    impls.extend(impl_);
                    tys.insert(using);
                }
            }
        }
        _ => panic!("unsupported data structure"),
    };

    let mut usings = TokenStream2::new();
    for ty in tys {
        usings.extend(quote!(use ac::#ty;));
    }

    let name = &ast.ident;
    let name_module = Ident::new(&name.to_string().to_snake_case(), Span::call_site());

    let gen = quote! {
        // impl Ac for #name {}
        pub mod #name_module {
            #usings
            use super::#name;

            #defines
            impl #name {
                #impls
            }
        }
    };

    // panic!("{}", gen.to_string());

    gen.into()
}
