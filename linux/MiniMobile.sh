#!/bin/sh
# MiniMobile - 한국 피처폰 게임(WIPI / SK-VM / BREW / J2ME) 에뮬레이터.
# 리눅스 PC용 실행 스크립트: 이 폴더에서 창 모드로 엽니다.
cd "$(dirname "$(readlink -f "$0")")" || exit 1
export RUST_LOG="${RUST_LOG:-warn}"
exec ./minimobile --windowed "$@"
