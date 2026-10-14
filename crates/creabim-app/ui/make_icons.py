"""Make icons.js from the lucide-static icons folder: make_icons.py ICONS_DIR > icons.js"""
import re, sys, json
src=sys.argv[1].rstrip('/')+'/'
names="""mouse-pointer-2 brick-wall door-open app-window layers layers-2 box scissors-line-dashed building building-2 ruler tag pen-tool slash undo-2 redo-2 move rotate-cw scaling copy-plus copy scissors clipboard-paste trash-2 eye-off focus scan-eye eye folder chevron-right chevron-down search file magnet grid-3x3 square-dashed crosshair check arrow-left layers-3 columns-2 scan sticky-note frame panel-left hash axis-3d circle-dot move-3d spline file-plus folder-open save download upload link sheet table-2 refresh-cw cuboid house map-pin layout-dashboard square-dashed-mouse-pointer git-commit-horizontal ellipsis sun moon panel-right panel-bottom maximize-2 minus diamond palette map file-text""".split()
# for the 3D view's buttons and the command palette
names+="""zoom-in zoom-out rotate-3d""".split()
# for the annotation tools
names+="""drafting-compass radius diameter message-square-text arrow-up-right""".split()
# for the building element tools
names+="""rectangle-vertical rectangle-horizontal tent chart-no-axes-column-increasing fence corner-down-right""".split()
# snap markers, drawn like the markers in the views (not from Lucide)
CUSTOM={
 "snap-endpoint": '<rect x="6" y="6" width="12" height="12" />',
 "snap-midpoint": '<path d="M12 5.5 19.5 18.5h-15z" />',
 "snap-intersection": '<path d="m6 6 12 12M18 6 6 18" />',
 "snap-perpendicular": '<path d="M4 19h16M10 19V5M10 14h5v5" />',
 "snap-nearest": '<path d="M6 5h12L6 19h12z" />',
 "snap-centre": '<circle cx="12" cy="12" r="7" /><circle cx="12" cy="12" r="1" />',
 "snap-quadrant": '<path d="M12 4.5 19.5 12 12 19.5 4.5 12z" />',
 "snap-axis": '<path d="M3 12h18M12 3v18" stroke-dasharray="2.5 2.5" />',
}
out={}
for n in names:
    s=open(src+n+'.svg').read()
    s=re.sub(r'<!--.*?-->','',s,flags=re.S); inner=re.search(r'<svg[^>]*>(.*)</svg>', s, re.S).group(1)
    out[n]=re.sub(r'\s+',' ',inner).strip()
out.update(CUSTOM)
sys.stdout.write("// Icons from Lucide (https://lucide.dev), ISC License; see LICENSE-lucide.\n")
sys.stdout.write("window.ICONS = "+json.dumps(out, indent=0)+";\n")
