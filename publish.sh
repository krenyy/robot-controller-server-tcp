#!/bin/bash

shopt -s extglob
shopt -s globstar

cd "$(dirname "$0")" || exit

{
  echo "// Cargo.toml"
  sed -E 's/^/\/\/ /' <Cargo.toml

  for f in src/**/*.rs; do
    echo -e "\n// $f"
    cat "$f"
  done
} >out.rs
