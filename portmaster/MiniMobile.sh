#!/bin/bash
# MiniMobile - 한국 피처폰 게임(WIPI / SK-VM / BREW / J2ME) 에뮬레이터.
# PortMaster 포트 실행 스크립트.

XDG_DATA_HOME=${XDG_DATA_HOME:-$HOME/.local/share}

if [ -d "/opt/system/Tools/PortMaster/" ]; then
  controlfolder="/opt/system/Tools/PortMaster"
elif [ -d "/opt/tools/PortMaster/" ]; then
  controlfolder="/opt/tools/PortMaster"
elif [ -d "$XDG_DATA_HOME/PortMaster/" ]; then
  controlfolder="$XDG_DATA_HOME/PortMaster"
else
  controlfolder="/roms/ports/PortMaster"
fi

source $controlfolder/control.txt
[ -f "${controlfolder}/mod_${CFW_NAME}.txt" ] && source "${controlfolder}/mod_${CFW_NAME}.txt"
get_controls

GAMEDIR="/$directory/ports/minimobile"
cd "$GAMEDIR" || exit 1
> "$GAMEDIR/log.txt" && exec > >(tee "$GAMEDIR/log.txt") 2>&1

# The pad is read through SDL2 with the firmware's own mapping for it. No
# gptokeyb: the port reads the pad itself, and gptokeyb's SELECT+START would
# end the whole port where it means "back to the list" here.
export SDL_GAMECONTROLLERCONFIG="$sdl_controllerconfig"
# 자세한 로그가 필요하면 warn 대신 info로 바꾸세요.
export RUST_LOG="${RUST_LOG:-warn}"

$ESUDO chmod +x "$GAMEDIR/minimobile.${DEVICE_ARCH}"

if type pm_platform_helper >/dev/null 2>&1; then
  pm_platform_helper "$GAMEDIR/minimobile.${DEVICE_ARCH}"
fi

./minimobile.${DEVICE_ARCH} "$GAMEDIR/games"

if type pm_finish >/dev/null 2>&1; then
  pm_finish
else
  printf "\033c" > /dev/tty0 2>/dev/null
fi
