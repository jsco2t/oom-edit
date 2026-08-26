# oom-edit bundled-theme semantic mapping

These mappings are project-owned design decisions. The upstream files in
`upstream/` remain unmodified audit evidence; this document records which
reviewed upstream colors oom-edit assigns to its stable 18 semantic roles.

| Role | Catppuccin Mocha | Dracula | Nord | Solarized Dark | Tokyo Night |
| --- | --- | --- | --- | --- | --- |
| background | base `#1e1e2e` | Background `#282a36` | nord0 `#2e3440` | base03 `#002b36` | Night bg `#1a1b26` |
| background-alt | mantle `#181825` | Current Line `#44475a` | nord1 `#3b4252` | base02 `#073642` | Night bg_dark `#16161e` |
| gutter-background | mantle `#181825` | Current Line `#44475a` | nord1 `#3b4252` | base02 `#073642` | Night bg_dark `#16161e` |
| gutter-text | overlay1 `#7f849c` | Comment `#6272a4` | nord3 `#4c566a` | base01 `#586e75` | Storm fg_gutter `#3b4261` |
| gutter-text-active | text `#cdd6f4` | Foreground `#f8f8f2` | nord6 `#eceff4` | base1 `#93a1a1` | Storm fg_dark `#a9b1d6` |
| surface | surface0 `#313244` | Current Line `#44475a` | nord1 `#3b4252` | base02 `#073642` | Storm bg_highlight `#292e42` |
| surface-active | surface2 `#585b70` | Comment `#6272a4` | nord2 `#434c5e` | base01 `#586e75` | Storm blue7 `#394b70` |
| border | surface1 `#45475a` | Comment `#6272a4` | nord3 `#4c566a` | base01 `#586e75` | Storm terminal_black `#414868` |
| text | text `#cdd6f4` | Foreground `#f8f8f2` | nord4 `#d8dee9` | base0 `#839496` | Storm fg `#c0caf5` |
| text-muted | overlay1 `#7f849c` | Comment `#6272a4` | nord3 `#4c566a` | base01 `#586e75` | Storm comment `#565f89` |
| text-emphasis | rosewater `#f5e0dc` | Foreground `#f8f8f2` | nord6 `#eceff4` | base1 `#93a1a1` | Storm blue6 `#b4f9f8` |
| primary | mauve `#cba6f7` | Purple `#bd93f9` | nord8 `#88c0d0` | blue `#268bd2` | Storm blue `#7aa2f7` |
| secondary | lavender `#b4befe` | Pink `#ff79c6` | nord15 `#b48ead` | violet `#6c71c4` | Storm magenta `#bb9af7` |
| info | blue `#89b4fa` | Cyan `#8be9fd` | nord9 `#81a1c1` | cyan `#2aa198` | Storm cyan `#7dcfff` |
| success | green `#a6e3a1` | Green `#50fa7b` | nord14 `#a3be8c` | green `#859900` | Storm green `#9ece6a` |
| warning | yellow `#f9e2af` | Yellow `#f1fa8c` | nord13 `#ebcb8b` | yellow `#b58900` | Storm yellow `#e0af68` |
| error | red `#f38ba8` | Red `#ff5555` | nord11 `#bf616a` | red `#dc322f` | Storm red `#f7768e` |
| attention | peach `#fab387` | Orange `#ffb86c` | nord12 `#d08770` | orange `#cb4b16` | Storm orange `#ff9e64` |

TrueColor lowering uses these values exactly. ANSI-16 lowering is an explicit
oom-edit semantic mapping: structure uses black/white/gray/dark-gray; primary,
secondary, and info use blue/magenta/cyan as appropriate; success, warning,
error, and attention use green/yellow/red/yellow. Monochrome lowering remains
shared application behavior and never derives terminal colors from these RGB
values.
