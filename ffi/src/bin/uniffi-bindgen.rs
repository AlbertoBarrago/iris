//! Writes the Swift bindings for this crate: `scripts/build-ffi.sh` runs it.

fn main() {
    uniffi::uniffi_bindgen_main()
}
