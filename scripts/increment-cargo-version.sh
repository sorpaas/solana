#!/usr/bin/env bash
set -e

usage() {
  cat <<EOF
usage: $0 [major|minor|patch|-preXYZ]

Increments the Cargo.toml version.
A minor version increment is the default
EOF
  exit 0
}

here="$(dirname "$0")"
cd "$here"/..
source ci/semver_bash/semver.sh

readCargoVariable() {
  declare variable="$1"
  declare Cargo_toml="$2"

  while read -r name equals value _; do
    if [[ $name = "$variable" && $equals = = ]]; then
      echo "${value//\"/}"
      return
    fi
  done < <(cat "$Cargo_toml")
  echo "Unable to locate $variable in $Cargo_toml" 1>&2
}

# shellcheck disable=2044 # Disable 'For loops over find output are fragile...'
Cargo_tomls="$(find . -name Cargo.toml)"

# Collect the name of all the internal crates
crates=()
for Cargo_toml in $Cargo_tomls; do
  crates+=("$(readCargoVariable name "$Cargo_toml")")
done

# Read the current version
MAJOR=0
MINOR=0
PATCH=0
SPECIAL=""
semverParseInto "$(readCargoVariable version ./Cargo.toml)" MAJOR MINOR PATCH SPECIAL
[[ -n $MAJOR ]] || usage

currentVersion="$MAJOR.$MINOR.$PATCH$SPECIAL"
SPECIAL=""

# Figure out what to increment
case ${1:-minor} in
patch)
  PATCH=$((PATCH + 1))
  ;;
major)
  MAJOR=$((MAJOR+ 1))
  ;;
minor)
  MINOR=$((MINOR+ 1))
  ;;
-*)
  if [[ $1 =~ ^-[A-Za-z0-9]*$ ]]; then
    SPECIAL="$1"
  else
    echo "Error: Unsupported characters found in $1"
    exit 1
  fi
  ;;
*)
  echo "Error: unknown argument: $1"
  usage
  ;;
esac

newVersion="$MAJOR.$MINOR.$PATCH$SPECIAL"

# Update all the Cargo.toml files
for Cargo_toml in $Cargo_tomls; do
  # Set new crate version
  (
    set -x
    sed -e "s/^version = \"[^\"]*\"$/version = \"$newVersion\"/" -i '' "$Cargo_toml"
  )

  # Fix up the version references to other internal crates
  for crate in "${crates[@]}"; do
    (
      set -x
      sed -e "
        s/^$crate = .*path = \"\([^\"]*\)\".*\$/$crate = \{ path = \"\1\", version = \"$newVersion\" \}/
      " -i '' "$Cargo_toml"
    )
  done
done

echo "$currentVersion -> $newVersion"

exit 0
