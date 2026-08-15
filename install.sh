#!/bin/sh
set -eu

repository=${INVESTIGATOR_CLI_REPOSITORY:-arkadiuszspiewak/investigator}
version=latest
install_dir=${INVESTIGATOR_CLI_INSTALL_DIR:-"$HOME/.local/bin"}

usage() {
  printf '%s\n' 'Usage: install.sh [--version VERSION] [--install-dir DIRECTORY]'
  printf '%s\n' 'Installs investigator-cli from its GitHub release artifacts.'
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --version)
      [ "$#" -ge 2 ] || { printf '%s\n' 'error: --version requires a value' >&2; exit 2; }
      version=$2
      shift 2
      ;;
    --install-dir)
      [ "$#" -ge 2 ] || { printf '%s\n' 'error: --install-dir requires a value' >&2; exit 2; }
      install_dir=$2
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      printf 'error: unknown argument: %s\n' "$1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

case "$(uname -s)" in
  Linux) system=unknown-linux-gnu ;;
  Darwin) system=apple-darwin ;;
  *) printf 'error: unsupported operating system: %s\n' "$(uname -s)" >&2; exit 1 ;;
esac

case "$(uname -m)" in
  x86_64|amd64) architecture=x86_64 ;;
  arm64|aarch64) architecture=aarch64 ;;
  *) printf 'error: unsupported architecture: %s\n' "$(uname -m)" >&2; exit 1 ;;
esac

target="${architecture}-${system}"
archive="investigator-cli-${target}.tar.gz"

temporary_dir=$(mktemp -d)
trap 'rm -rf "$temporary_dir"' EXIT HUP INT TERM

download() {
  url=$1
  destination=$2
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$url" -o "$destination"
  elif command -v wget >/dev/null 2>&1; then
    wget -q "$url" -O "$destination"
  else
    printf '%s\n' 'error: curl or wget is required' >&2
    exit 1
  fi
}

if [ "$version" = latest ]; then
  releases_file="${temporary_dir}/releases.json"
  download "https://api.github.com/repos/${repository}/releases?per_page=100" "$releases_file"
  tag=$(sed -n 's/^[[:space:]]*"tag_name":[[:space:]]*"\(investigator-cli-v[^"[:space:]]*\)",*[[:space:]]*$/\1/p' "$releases_file" | head -n 1)
  if [ -z "$tag" ]; then
    printf 'error: no investigator-cli-v* release found in %s\n' "$repository" >&2
    exit 1
  fi
else
  case "$version" in investigator-cli-v*) tag=$version ;; *) tag="investigator-cli-v${version}" ;; esac
fi
release_url="https://github.com/${repository}/releases/download/${tag}"

printf 'Downloading %s for %s...\n' "$tag" "$target"
download "${release_url}/${archive}" "${temporary_dir}/${archive}"
download "${release_url}/checksums.txt" "${temporary_dir}/checksums.txt"

expected=$(awk -v name="$archive" '$2 == name {print $1}' "${temporary_dir}/checksums.txt")
[ -n "$expected" ] || { printf '%s\n' "error: checksum for ${archive} is missing" >&2; exit 1; }
if command -v sha256sum >/dev/null 2>&1; then
  actual=$(sha256sum "${temporary_dir}/${archive}" | awk '{print $1}')
else
  actual=$(shasum -a 256 "${temporary_dir}/${archive}" | awk '{print $1}')
fi
[ "$actual" = "$expected" ] || { printf '%s\n' 'error: downloaded archive checksum does not match' >&2; exit 1; }

tar -xzf "${temporary_dir}/${archive}" -C "$temporary_dir"
mkdir -p "$install_dir"
install -m 0755 "${temporary_dir}/investigator-cli" "${install_dir}/investigator-cli"
printf 'Installed investigator-cli to %s/investigator-cli\n' "$install_dir"
case ":$PATH:" in *":${install_dir}:"*) ;; *) printf 'Add %s to your PATH.\n' "$install_dir" ;; esac
