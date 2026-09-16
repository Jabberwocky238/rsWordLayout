#!/bin/sh
# 四个 prepare 脚本共用的小工具。用 POSIX sh，不依赖 bash。
#
# 约定：脚本只做两件事——**检查**与**报告**。凡是需要 root 的安装动作一律只打印命令，
# 由人自己执行。理由是包管理器的选择、镜像源、以及是否愿意装几百 MB 的驱动开发包，
# 都不该由脚本替用户决定。

set -eu

RED=''; GRN=''; YLW=''; DIM=''; RST=''
if [ -t 1 ]; then
  RED=$(printf '\033[31m'); GRN=$(printf '\033[32m')
  YLW=$(printf '\033[33m'); DIM=$(printf '\033[2m'); RST=$(printf '\033[0m')
fi

MISSING=0

say()  { printf '%s\n' "$*"; }
head1() { printf '\n%s== %s ==%s\n' "$DIM" "$*" "$RST"; }
ok()   { printf '  %s✓%s %s\n' "$GRN" "$RST" "$*"; }
warn() { printf '  %s!%s %s\n' "$YLW" "$RST" "$*"; }
bad()  { printf '  %s✗%s %s\n' "$RED" "$RST" "$*"; MISSING=$((MISSING+1)); }
hint() { printf '      %s%s%s\n' "$DIM" "$*" "$RST"; }

have() { command -v "$1" >/dev/null 2>&1; }

# 检查一个命令是否存在；缺失时打印安装提示。
need_cmd() {
  _c=$1; shift
  if have "$_c"; then
    ok "$_c"
  else
    bad "$_c 缺失"
    [ $# -gt 0 ] && hint "$*"
  fi
}

# 探测发行版的包管理器，供提示用。
pkg_hint() {
  if have apt-get; then printf 'sudo apt-get install -y %s' "$1"
  elif have dnf;  then printf 'sudo dnf install -y %s' "$2"
  elif have pacman; then printf 'sudo pacman -S --needed %s' "$3"
  else printf '请用你的包管理器安装：%s' "$1"
  fi
}

finish() {
  printf '\n'
  if [ "$MISSING" -eq 0 ]; then
    printf '%s全部就绪%s\n' "$GRN" "$RST"
    exit 0
  fi
  printf '%s缺 %s 项%s：按上面的提示安装后重跑本脚本。\n' "$YLW" "$MISSING" "$RST"
  printf '%s脚本不会替你执行需要 root 的命令。%s\n' "$DIM" "$RST"
  exit 1
}
