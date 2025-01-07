extern crate proc_macro;

use proc_macro::{Ident, TokenStream};
use syn::{parse_macro_input, Token};

use syn::parse::{Parse, ParseStream};

struct MacroInput {
    crate_name: String,
    version: String,
    patches: Vec<String>,
}

impl Parse for MacroInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let struct_expr = syn::ExprStruct::parse(input)?;

        let crate_name = struct_expr.path.get_ident().unwrap().to_string();

        let mut version = String::new();
        let mut patches = Vec::new();

        for field in struct_expr.fields {
            if let syn::Member::Named(member) = field.member {
                let name = member.to_string();
                match name.as_str() {
                    "version" => {
                        if !version.is_empty() {
                            panic!("version defined multiple times");
                        }
                        if let syn::Expr::Lit(syn::ExprLit {
                            lit: syn::Lit::Str(x),
                            ..
                        }) = field.expr
                        {
                            version = x.value();
                        } else {
                            panic!("only string literal is allowd for version")
                        }
                    }
                    "patches" => {
                        if let syn::Expr::Array(exprs) = field.expr {
                            for expr in exprs.elems {
                                if let syn::Expr::Lit(syn::ExprLit {
                                    lit: syn::Lit::Str(x),
                                    ..
                                }) = expr
                                {
                                    patches.push(x.value());
                                } else {
                                    panic!("only Array of strings is allowed for patches")
                                }
                            }
                        } else {
                            panic!("only Array is allowed for patches")
                        }
                    }
                    x => panic!("unknown member {x}"),
                }
            } else {
                panic!("?!");
            }
        }

        Ok(MacroInput {
            crate_name,
            version,
            patches,
        })
    }
}

#[proc_macro]
pub fn crate_patcher(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as MacroInput);

    "fn answer() -> u32 { 42 }".parse().unwrap()
}
