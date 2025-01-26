#![allow(unused_variables)]

extern crate proc_macro;

use std::path::PathBuf;
use std::str::FromStr;

use file_lock::{FileLock, FileOptions};
use flate2::read::GzDecoder;
use proc_macro::TokenStream;
use reqwest::StatusCode;
use syn::parse_macro_input;

use syn::parse::{Parse, ParseStream};
use tar::Archive;
use toml_edit::Table;

struct MacroInput {
    crate_name: String,
    version: String,
    // patches: Vec<String>,
}

impl Parse for MacroInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let struct_expr = syn::ExprStruct::parse(input)?;

        let crate_name = struct_expr.path.get_ident().unwrap().to_string();

        let mut version = String::new();
        // let mut patches = Vec::new();

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
                    // "patches" => {
                    //     if let syn::Expr::Array(exprs) = field.expr {
                    //         for expr in exprs.elems {
                    //             if let syn::Expr::Lit(syn::ExprLit {
                    //                 lit: syn::Lit::Str(x),
                    //                 ..
                    //             }) = expr
                    //             {
                    //                 patches.push(x.value());
                    //             } else {
                    //                 panic!("only Array of strings is allowed for patches")
                    //             }
                    //         }
                    //     } else {
                    //         panic!("only Array is allowed for patches")
                    //     }
                    // }
                    x => panic!("unknown member {x}"),
                }
            } else {
                panic!("?!");
            }
        }

        Ok(MacroInput {
            crate_name,
            version,
            // patches,
        })
    }
}
use std::io::Write;

#[proc_macro]
pub fn crate_patcher(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as MacroInput);

    let dir: PathBuf = std::env::var("CARGO_MANIFEST_DIR").unwrap().into();
    let w_dir = dir.join("./target/crate_patcher/");
    if !w_dir.exists() {
        std::fs::create_dir_all(&w_dir).unwrap();
    }

    let options = FileOptions::new().write(true).create(true).append(true);

    let filelock = match FileLock::lock(w_dir.join("./crate-patcher.lock"), false, options) {
        Ok(lock) => lock,
        Err(err) => {
            return r#"include!("./lib.crate.rs");"#.parse().unwrap();
        }
    };

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
    let lib_src_root = {
        let original = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap();
        let crate_toml =
            std::fs::read_to_string(w_dir.join(&original_crate_dir).join("Cargo.toml")).unwrap();

        let mut doc = original.parse::<toml_edit::DocumentMut>().unwrap();
        let crate_doc = crate_toml.parse::<toml_edit::DocumentMut>().unwrap();

        for table in ["dev-dependencies", "features", "dependencies"] {
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

        if doc.contains_key("lib") {
            PathBuf::from_str(
                doc["lib"]["path"]
                    .as_str()
                    .expect("[lib].path must be string"),
            )
            .unwrap()
            .parent()
            .map(|x| x.to_str().unwrap().to_owned())
            .unwrap_or_default()
        } else {
            "src".to_owned()
        }
    };

    // prepare for patching
    if !dir.join("./src/.gitignore").exists() {
        std::fs::write(dir.join("./src/.gitignore"), "*\n!lib.rs").unwrap();
    }
    if !dir.join("./patches").exists() {
        std::fs::create_dir_all(dir.join("./patches")).unwrap()
    }

    let o_crate_dir = w_dir.join(original_crate_dir);
    // sync code and apply patches
    let crate_files: Vec<_> = glob::glob(&format!(
        "{}/**/*",
        o_crate_dir.join(&lib_src_root).to_str().unwrap()
    ))
    .unwrap()
    .filter_map(Result::ok)
    .filter(|x| x.is_file() && x.file_name().unwrap().to_str().unwrap() != "Cargo.toml")
    .map(|x| x.strip_prefix(o_crate_dir.clone()).unwrap().to_path_buf())
    .collect();

    let current_files: Vec<_> = glob::glob(&format!("{}/src/**/*", dir.to_str().unwrap()))
        .unwrap()
        .filter_map(Result::ok)
        .filter(|x| x.is_file())
        .map(|x| x.strip_prefix(dir.join("./src")).unwrap().to_path_buf())
        .collect();

    // let current_patches: Vec<_> = glob::glob(&format!("{}/patches/*.patch", dir.to_str().unwrap()))
    //     .unwrap()
    //     .filter_map(Result::ok)
    //     .filter(|x| x.is_file())
    //     .collect();

    for crate_file in crate_files.iter() {
        let target_crate_file = crate_file
            .strip_prefix(lib_src_root.as_str())
            .unwrap()
            .to_path_buf();
        let _ =
            std::fs::create_dir_all(dir.join("./src").join(&target_crate_file).parent().unwrap());
        let original_content = {
            match std::fs::read_to_string(o_crate_dir.join(crate_file)) {
                Ok(x) => x,
                // probabely binary, just copy it over
                Err(_) => {
                    if !current_files.contains(&target_crate_file) {
                        let from = o_crate_dir.join(crate_file);
                        let to = dir.join("./src").join(target_crate_file);
                        if std::fs::copy(&from, &to).is_err() {
                            panic!("copy failed {:?} -> {:?}", from, to);
                        }
                    }
                    continue;
                }
            }
        };

        let patch_file_name = crate_file_to_patch_file(&crate_file.to_path_buf());

        let content = if dir.join(&patch_file_name).exists() {
            let patch_c = std::fs::read_to_string(dir.join(&patch_file_name)).unwrap();
            let patch = diffy::Patch::from_str(patch_c.as_str()).unwrap();
            diffy::apply(&original_content, &patch).unwrap()
        } else {
            original_content.clone()
        };

        let current_files_name = if crate_file.file_name().unwrap().to_str().unwrap() == "lib.rs" {
            PathBuf::from_str("lib.crate.rs").unwrap()
        } else {
            target_crate_file.clone()
        };
        let target = dir.join("./src").join(&current_files_name);

        if !current_files.contains(&current_files_name) {
            std::fs::write(target, content).unwrap();
        } else {
            let current_file_content = std::fs::read_to_string(target).unwrap();
            if current_file_content == content {
                // FIXME: i don't know why this is buggy
                // let _ = std::fs::remove_file(dir.join(patch_file_name));
                continue;
            }

            let diff = diffy::create_patch(&original_content, &current_file_content);

            std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .open(dir.join(patch_file_name))
                .expect("?")
                .write_all(diff.to_string().as_bytes())
                .expect("??");
        }
    }

    r#"include!("./lib.crate.rs");"#.parse().unwrap()
}

fn crate_file_to_patch_file(crate_file_name: &PathBuf) -> PathBuf {
    format!(
        "patches/{}.patch",
        crate_file_name.to_str().unwrap().replace("/", "--")
    )
    .into()
}
