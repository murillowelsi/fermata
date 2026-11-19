# The installer disk image (F-14c) for dmgbuild (https://github.com/dmgbuild/dmgbuild, MIT): the same window as the
# Finder layout in make_mac_app.sh, written straight into .DS_Store, so it builds without a graphical session
# (GitHub Actions). Values copied from the approved installer: a 720x460 window (720x488 with the title bar) with the
# Traço background, Fermata on the left and Applications on the right, icons of 128 pt, labels of 13 pt.
# Run from the repository root: dmgbuild -s assets/dmg/dmgbuild-settings.py Fermata dist/Fermata-Installer.dmg
import os.path

app = defines.get("app", "dist/Fermata.app")  # noqa: F821 (dmgbuild provides `defines`)

volume_name = "Fermata"
format = "UDZO"
compression_level = 9
filesystem = "HFS+"
files = [app]
symlinks = {"Applications": "/Applications"}
icon = "assets/dmg/VolumeIcon.icns"
background = "assets/dmg/fundo.tiff"

show_status_bar = False
show_tab_view = False
show_toolbar = False
show_pathbar = False
show_sidebar = False
sidebar_width = 180
window_rect = ((200, 442), (720, 488))
default_view = "icon-view"
show_icon_preview = True
include_icon_view_settings = True
include_list_view_settings = False

arrange_by = None
grid_offset = (0, 0)
grid_spacing = 100
label_pos = "bottom"
text_size = 13
icon_size = 128
icon_locations = {
    os.path.basename(app): (190, 250),
    "Applications": (530, 250),
}
