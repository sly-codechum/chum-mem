#!/usr/bin/env sh
set -eu

target_from_arch() {
  case "$1" in
    arm64 | aarch64)
      printf '%s\n' "aarch64-apple-darwin"
      ;;
    x86_64)
      printf '%s\n' "x86_64-apple-darwin"
      ;;
    *)
      return 1
      ;;
  esac
}

target="${RUST_LLD_DARWIN_TARGET:-}"
previous=""

for arg in "$@"; do
  if [ "$previous" = "-arch" ]; then
    target="$(target_from_arch "$arg" || printf '%s' "$target")"
    previous=""
    continue
  fi

  if [ "$arg" = "-arch" ]; then
    previous="-arch"
  else
    previous=""
  fi
done

if [ -z "$target" ]; then
  target="$(rustc -Vv | awk '/^host:/ {print $2}')"
fi

sysroot="${RUSTC_SYSROOT:-$(rustc --print sysroot)}"
lld="$sysroot/lib/rustlib/$target/bin/gcc-ld/ld64.lld"

if [ ! -x "$lld" ]; then
  printf '%s\n' "error: ld64.lld not found for Rust target '$target': $lld" >&2
  printf '%s\n' "help: install the target with 'rustup target add $target' or set RUST_LLD_DARWIN_TARGET." >&2
  exit 1
fi

exec clang -fuse-ld="$lld" "$@"
