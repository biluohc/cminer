#![recursion_limit = "128"]
#[macro_use]
extern crate darling;
extern crate proc_macro;

use crate::proc_macro::TokenStream;
use proc_macro2::{Ident, Span, TokenStream as TokenStream2};

use darling::FromField;
use heck::{ShoutySnakeCase, SnakeCase};
use quote::quote;
use syn::Data;

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

fn impl_ac_macro(ast: &syn::DeriveInput) -> TokenStream {
    let mut defines = TokenStream2::new();
    let mut usings = TokenStream2::new();

    match &ast.data {
        Data::Struct(data) => {
            for (_idx, field) in data.fields.iter().enumerate() {
                let ac = AcAttrs::from_field(&field);

                match (field.ident.as_ref(), ac) {
                    (Some(fident), Ok(ac)) if !ac.skip => {
                        let default = ac.default.unwrap_or(0);
                        let fname = Ident::new(&fident.to_string().to_shouty_snake_case(), Span::call_site());
                        let define = quote! {
                            ///pub static #fname: Counter = Counter::new(#default);
                            pub static #fname: Counter = Counter::new(#default);
                        };
                        let using = quote! {
                            pub fn #fident() -> &'static Counter {
                                &#fname
                            }
                        };

                        defines.extend(define);
                        usings.extend(using);
                    }
                    _ => {}
                }
            }
        }
        _ => panic!("unsupported data structure"),
    };

    let name = &ast.ident;
    let name_module = Ident::new(&name.to_string().to_snake_case(), Span::call_site());

    let gen = quote! {
        impl Ac for #name {}
        pub mod #name_module {
            use ac::Counter;
            use super::#name;

            #defines
            impl #name {
                #usings
            }
        }
    };

    // panic!("{}", gen.to_string());

    gen.into()
}
