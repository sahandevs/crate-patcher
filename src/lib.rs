extern crate proc_macro;

use std::env;
use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;
use proc_macro::{Ident, TokenStream};
use reqwest::StatusCode;
use syn::{parse_macro_input, Token};

use syn::parse::{Parse, ParseStream};
use tar::Archive;
use toml_edit::Table;

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
use std::fs::File;
use std::io::Read;
use std::io::{self, Write};

#[proc_macro]
pub fn crate_patcher(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as MacroInput);

    let dir: PathBuf = std::env::var("CARGO_MANIFEST_DIR").unwrap().into();
    let w_dir = dir.join("./target/crate_patcher/");
    if !w_dir.exists() {
        std::fs::create_dir_all(&w_dir).unwrap();
    }

    let crate_file_name = format!("{}-{}.crate", input.crate_name, input.version);

    if !w_dir.join(&crate_file_name).exists() {
        let resp = reqwest::blocking::get(format!(
            "https://static.crates.io/crates/{}/{crate_file_name}",
            input.crate_name
        ))
        .unwrap();
        if resp.status() != StatusCode::OK {
            panic!("couldn'd download the crate");
        }

        let body: Vec<_> = resp.bytes().unwrap().into();
        let mut target_file = std::fs::File::create(w_dir.join(&crate_file_name)).unwrap();
        // gzip compressed data
        std::io::copy(&mut body.as_slice(), &mut target_file).unwrap();
    }

    let original_crate_dir = format!("{}-{}", input.crate_name, input.version);
    if !w_dir.join(&original_crate_dir).exists() {
        let tar_gz = std::fs::File::open(w_dir.join(&crate_file_name)).unwrap();
        let tar = GzDecoder::new(tar_gz);
        let mut archive = Archive::new(tar);
        archive.unpack(&w_dir).unwrap();
    }

    // update Cargo.toml
    {
        let original = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap();
        let crate_toml =
            std::fs::read_to_string(w_dir.join(&original_crate_dir).join("Cargo.toml")).unwrap();

        let mut doc = original.parse::<toml_edit::Document>().unwrap();
        let crate_doc = crate_toml.parse::<toml_edit::Document>().unwrap();

        for table in ["dev-dependencies", "features", "dependencies", "lib", "bin"] {
            if let Some(toml_edit::Item::Table(x)) = crate_doc.get(table) {
                if !doc.contains_key(&table) {
                    doc.insert(&table, toml_edit::Item::Table(Table::new()));
                }
                let doc_t = doc[table].as_table_mut().unwrap();
                for (key, val) in x {
                    if !doc_t.contains_key(&key) {
                        doc_t.insert(key, val.clone());
                    }
                }
            }
        }

        std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .open(dir.join("Cargo.toml"))
            .expect("?")
            .write_all(doc.to_string().as_bytes())
            .expect("??");
    }

    "fn answer() -> u32 { 42 }".parse().unwrap()
}
