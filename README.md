## Build

Targeting LiveSplit's Auto Splitter Runtime (asr) requires adding a wasm target to your toolchain:

- `$ rustup target add wasm32-unknown-unknown`

To target WASM when building:

- `$ cargo build --release --target wasm32-unknown-unknown`

Find the final WASM module under `./target/wasm32-unknown-unknown/release/cosmic-shake-auto-splitter.wasm`

## Cheat Table

`CosmicShake-Win64-Shipping.CT` is a Cheat Engine table that contains a lot of reverse engineered pointer paths that are used by the autosplitter and others that may or may not be useful in the future. There are no actual cheats implemented here, it is purely for reverse engineering purposes.

#### License

<sup>
Licensed under either of <a href="LICENSE-APACHE">Apache License, Version
2.0</a> or <a href="LICENSE-MIT">MIT license</a> at your option.       
</sup>

<br>

<sub>
Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in cosmic-shake-auto-splitter by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.    
</sub>
