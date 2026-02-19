#!/bin/bash
set -e
tools/bump.sh
tools/test.sh
v=$(grep '^version = ' pyproject.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
git commit -am "Release v$v"
git tag "v$v"
git push origin main --tags
