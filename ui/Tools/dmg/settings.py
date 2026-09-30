# dmgbuild settings for the Swarm installer window (Tools/make-dmg.sh).
#
# The layout goes straight into the volume's .DS_Store, so it does not depend on the Finder
# settings of the Mac that built it. Coordinates are icon centres in the 540x300 content area.
#
# A missing -D define raises KeyError on purpose: the build stops rather than package a
# half-configured image.
import os

app = defines["app"]

format = "UDZO"
compression_level = 9
filesystem = "HFS+"

files = [app]
symlinks = {"Applications": "/Applications"}

# The frame includes the 32 pt macOS 26 title bar.
window_rect = ((200, 120), (540, 332))
default_view = "icon-view"
show_status_bar = False
show_tab_view = False
show_toolbar = False
show_pathbar = False
show_sidebar = False

icon_size = 128
text_size = 12
label_pos = "bottom"
arrange_by = None

icon_locations = {
    os.path.basename(app): (140, 150),
    "Applications": (400, 150),
}
