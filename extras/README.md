# tabs

`tabs` is a small terminal utility for everyday score chores. It uses
one-based bar and track numbers and never overwrites an existing output.

```sh
cargo run --manifest-path extras/Cargo.toml -- info song.gp
cargo run --manifest-path extras/Cargo.toml -- tab song.gp --start 3 --end 6
cargo run --manifest-path extras/Cargo.toml -- transpose song.gp transposed.gp 2
cargo run --manifest-path extras/Cargo.toml -- slice song.gp excerpt.gp 4 8
cargo run --manifest-path extras/Cargo.toml -- midi song.gp song.mid
```
