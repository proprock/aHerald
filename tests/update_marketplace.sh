#!/bin/sh
# Contract test for scripts/update-marketplace.sh: a catalog file, a release tag and a commit
# in; the same catalog with the aherald entry pinned to that release out.
# Run from the repository root: sh tests/update_marketplace.sh
set -eu

script=scripts/update-marketplace.sh
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
failures=0
sha1=1111111111111111111111111111111111111111
sha2=2222222222222222222222222222222222222222

fail() {
  echo "FAIL: $1" >&2
  failures=$((failures + 1))
}

# A catalog shaped like proprock/marketplace: another plugin that must never change.
fresh_catalog() {
  cat > "$1" <<'JSON'
{
  "name": "proprock",
  "owner": {
    "name": "proprock"
  },
  "plugins": [
    {
      "name": "kaboost",
      "source": {
        "source": "git-subdir",
        "url": "https://github.com/proprock/kaBoost.git",
        "path": "plugins/kaboost",
        "ref": "v0.2.0",
        "sha": "9c29997939346caf67ecd67e2d1b0d03839129b4"
      },
      "description": "Portable engineering rules and focused workflows"
    }
  ]
}
JSON
}

kaboost_of() { jq -c '.plugins[] | select(.name == "kaboost")' "$1"; }
aherald_of() { jq -c '.plugins[] | select(.name == "aherald")' "$1"; }
count_of() { jq -r --arg n "$2" '[.plugins[] | select(.name == $n)] | length' "$1"; }

catalog="$work/catalog.json"
fresh_catalog "$catalog"
kaboost_before=$(kaboost_of "$catalog")

# 1. A missing entry is added, pinned by tag and commit, and the other plugin is untouched.
sh "$script" "$catalog" v0.16.0 "$sha1" || fail "adding the entry exited non-zero"
[ "$(count_of "$catalog" aherald)" = 1 ] || fail "expected exactly one aherald entry"
[ "$(jq -r '.plugins[] | select(.name == "aherald") | .source.source' "$catalog")" = git-subdir ] ||
  fail "the source type is not git-subdir"
[ "$(jq -r '.plugins[] | select(.name == "aherald") | .source.url' "$catalog")" = \
  https://github.com/proprock/aHerald.git ] || fail "the source url is wrong"
[ "$(jq -r '.plugins[] | select(.name == "aherald") | .source.path' "$catalog")" = plugins/aherald ] ||
  fail "the plugin path is wrong"
[ "$(jq -r '.plugins[] | select(.name == "aherald") | .source.ref' "$catalog")" = v0.16.0 ] ||
  fail "the ref is not the tag"
[ "$(jq -r '.plugins[] | select(.name == "aherald") | .source.sha' "$catalog")" = "$sha1" ] ||
  fail "the sha is not the given commit"
[ "$(kaboost_of "$catalog")" = "$kaboost_before" ] || fail "the kaboost entry changed"

# 2. The same release again changes nothing, byte for byte.
cp "$catalog" "$work/before.json"
sh "$script" "$catalog" v0.16.0 "$sha1" || fail "a repeated run exited non-zero"
cmp -s "$catalog" "$work/before.json" || fail "a repeated run changed the file"

# 3. A newer release replaces the pin and does not add a second entry.
sh "$script" "$catalog" v0.17.0 "$sha2" || fail "bumping the entry exited non-zero"
[ "$(count_of "$catalog" aherald)" = 1 ] || fail "a bump left two aherald entries"
[ "$(jq -r '.plugins[] | select(.name == "aherald") | .source.ref' "$catalog")" = v0.17.0 ] ||
  fail "the bump did not move the ref"
[ "$(jq -r '.plugins[] | select(.name == "aherald") | .source.sha' "$catalog")" = "$sha2" ] ||
  fail "the bump did not move the sha"
[ "$(kaboost_of "$catalog")" = "$kaboost_before" ] || fail "the bump changed the kaboost entry"
[ "$(jq -r '.plugins | map(.name) | join(",")' "$catalog")" = kaboost,aherald ] ||
  fail "the plugin order changed"

# 4. Bad input is refused with a message and leaves the file as it was.
refuse() {
  name=$1
  shift
  cp "$catalog" "$work/before.json"
  if sh "$script" "$@" 2> "$work/err.txt"; then
    fail "$name: expected a non-zero exit"
  fi
  [ -s "$work/err.txt" ] || fail "$name: expected a message on stderr"
  cmp -s "$catalog" "$work/before.json" || fail "$name: the file changed"
}
refuse "a branch name instead of a tag" "$catalog" main "$sha1"
refuse "a short version" "$catalog" v1.2 "$sha1"
refuse "a pre-release tag" "$catalog" v1.2.3-rc.1 "$sha1"
refuse "a short sha" "$catalog" v0.18.0 abc123
refuse "a non-hex sha" "$catalog" v0.18.0 zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz
refuse "a missing argument" "$catalog" v0.18.0
refuse "a missing catalog" "$work/absent.json" v0.18.0 "$sha1"
printf '{ not json' > "$work/broken.json"
cp "$work/broken.json" "$work/broken.before"
if sh "$script" "$work/broken.json" v0.18.0 "$sha1" 2> /dev/null; then
  fail "a broken catalog: expected a non-zero exit"
fi
cmp -s "$work/broken.json" "$work/broken.before" || fail "a broken catalog was rewritten"

if [ "$failures" -ne 0 ]; then
  echo "$failures check(s) failed" >&2
  exit 1
fi
echo "update_marketplace: all checks passed"
