#!/bin/sh
# design.md §1.1: sim は bevy を import しない。
# 破れ始めたら workspace 2クレートに割る合図。コメント中の言及は見ない。
if grep -rnE '^\s*(pub )?use +bevy|bevy::' src/sim; then
    echo "sim 層が bevy を参照しています（design.md §1.1）" >&2
    exit 1
fi
echo "ok: sim は bevy 非依存"
