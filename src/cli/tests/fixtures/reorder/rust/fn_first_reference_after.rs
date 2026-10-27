//! Helpers follow their first calls when possible, after their callers.
//!
//! `a_run` comes first alphabetically and calls `z` before `a`.
//! Calling `z` again changes nothing; `deep` follows `a` and `unrelated` is last.

pub fn a_run() {
    z();
    a();
    z();
}

pub fn z_run() {
    a();
    z();
}

fn z() {
    deep();
}

fn a() {}

fn deep() {}

fn unrelated() {}
