"""Make icons.js from the lucide-static icons folder: make_icons.py ICONS_DIR > icons.js"""
import re, sys, json
src=sys.argv[1].rstrip('/')+'/'
names="""mouse-pointer-2 brick-wall door-open app-window layers layers-2 box scissors-line-dashed building building-2 ruler tag pen-tool slash undo-2 redo-2 move rotate-cw scaling copy-plus copy scissors clipboard-paste trash-2 eye-off focus scan-eye eye folder chevron-right chevron-down search file magnet grid-3x3 square-dashed crosshair check arrow-left layers-3 columns-2 scan sticky-note frame panel-left hash axis-3d circle-dot move-3d spline file-plus folder-open save download upload link sheet table-2 refresh-cw cuboid house map-pin layout-dashboard square-dashed-mouse-pointer git-commit-horizontal ellipsis sun moon panel-right panel-bottom maximize-2 minus diamond palette map file-text""".split()
# for the 3D view's buttons and the command palette
names+="""zoom-in zoom-out rotate-3d""".split()
# for the single view layout
names+="""square""".split()
# for the annotation tools
names+="""drafting-compass radius diameter message-square-text arrow-up-right""".split()
# for the building element tools
names+="""rectangle-vertical rectangle-horizontal tent fence corner-down-right""".split()
# flipping a section or elevation round
names+="""flip-horizontal-2""".split()
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
 # the Level tool and the project browser's group buttons
 "level": '<path d="M2 14h12" stroke-dasharray="3 2" /> <circle cx="18" cy="14" r="3.5" /> <path d="M18 10.5V14h3.5M18 14v3.5h-3.5" /> <path d="M3 7h9" />',
 "plus": '<path d="M5 12h14M12 5v14" />',
 # the project browser's sort button, and alignment tracking in the snap list
 "sort-by-level": '<path d="M3 6h8M3 12h6M3 18h4" /> <path d="M17 4v16M13 16l4 4 4-4" />',
 "snap-track": '<path d="M3 8h18M16 3v18" stroke-dasharray="2.5 2.5" /> <circle cx="16" cy="8" r="2" />',
 # the Align tool, temporary guides, annotation styles and sheet templates
 "align-tool": '<path d="M4 3v18" stroke-dasharray="2.5 2" /> <rect x="8" y="5" width="12" height="5" /> <rect x="8" y="14" width="7" height="5" />',
 "snap-guides": '<path d="M2 16h20M7 21 19 3" stroke-dasharray="2.5 2" /> <circle cx="13.7" cy="12" r="2.5" />',
 "anno-styles": '<path d="M3 17h12" /> <path d="m3 14 3 6M12 14l3 6" /> <circle cx="18" cy="17" r="2" /> <path d="M5 4h14M12 4v7" />',
 "sheet-template": '<rect x="3" y="4" width="18" height="16" /> <path d="M11 14h10M11 14v6M16 14v6" />',
 # X-ray and hidden elements: a solid with what is inside it, dashed
 # the Stair tool: a flight seen from the side, its handrail on posts
 "stair": '<path d="M3 21h4v-4h4v-4h4V9h4V5h2v16z" /> <path d="M3 14 14 3M7 17v-7M11 13V6" />',
 "xray":'<rect x="3" y="4" width="18" height="16" rx="2" /> <rect x="8" y="9" width="8" height="6" stroke-dasharray="2 2" />',
}
out={}
for n in names:
    s=open(src+n+'.svg').read()
    s=re.sub(r'<!--.*?-->','',s,flags=re.S); inner=re.search(r'<svg[^>]*>(.*)</svg>', s, re.S).group(1)
    out[n]=re.sub(r'\s+',' ',inner).strip()
out.update(CUSTOM)
sys.stdout.write("// Icons from Lucide (https://lucide.dev), ISC License; see LICENSE-lucide.\n")
sys.stdout.write("window.ICONS = "+json.dumps(out, indent=0)+";\n")
