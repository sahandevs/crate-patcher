/// https://doc.rust-lang.org/nightly/cargo/reference/registry-web-api.html

/// 1 - user defines this:

// src/lib.rs:

easy_patcher! {
   crate = "deno_core",
   version = "0.1.0",
   patches = ["patch_1", "patch_2"]
}


/* on first rust:

- downlaod the crate from https://doc.rust-lang.org/nightly/cargo/reference/registry-web-api.html and cache it
- exctract everything inside the src folder (gitignored)
- update the Cargo.toml if it's not updated
- rename lib.rs to __original__lib.rs
- apply all patches in patches (and somewhere store what did we patched)
- generate a include("./__original__lib.rs") instead of easy_patcher.

on next runs:

- check if applied patches are different from what we previously patched
-- start from original crate, start applying patches until all shared patches are applied
-- remove the patch files which were removed
-- if a new entry is added, use the name to create diff from current change v (crate + appleid patches - removed patches)
-- if no new entry added but there are still changes, do nothing (maybe emit a warning)
*/