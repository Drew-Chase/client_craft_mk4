set windows-shell := ["powershell.exe", "-NoProfile", "-NoLogo", "-Command"]
set shell := ["bash", "-c"]

default:
    @just --list

bench:
    @cargo bench ccmk4 --release

build:
    @cargo build --release

unique_crafting_types:
    @cargo run --example unique_crafting_types

fetch_recipes:
    @cargo run --package fetch_minecraft_recipes --release

test:
    @cargo test --release
