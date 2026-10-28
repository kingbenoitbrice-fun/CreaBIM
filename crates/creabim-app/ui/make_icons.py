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
# the Help menu and the Help panel
names+="""circle-help book-open keyboard rocket x arrow-right info""".split()
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
 # the Stair tool: a flight seen from the side, with its handrail
 "stair": '<path d="M3 21h4v-4h4v-4h4V9h4V5h2v16z" /> <path d="M3 14 12 5" />',
 # X-ray and hidden elements: a see-through (dashed) wall with the solid
 # beam inside it showing, unlike the Wall opening tool's solid frame
 "xray": '<rect x="7" y="2.5" width="10" height="19" rx="1" stroke-dasharray="2.5 2" /> <path d="M2 10h20v4H2z" fill="currentColor" fill-opacity=".35" />',
 # how the 3D view draws: shaded with edges, hidden line, wireframe
 "render-edges": '<path d="M3 9h12v12H3z" fill="currentColor" fill-opacity=".3" /> <path d="m3 9 6-6h12v12l-6 6M15 9l6-6" fill="currentColor" fill-opacity=".15" />',
 "render-hidden-line": '<rect x="3" y="9" width="12" height="12" /> <path d="m3 9 6-6h12v12l-6 6M15 9l6-6" />',
 "render-wireframe": '<rect x="3" y="9" width="12" height="12" /> <path d="m3 9 6-6h12v12l-6 6M15 9l6-6" /> <path d="M9 3v12h12M9 15l-6 6" stroke-dasharray="2 1.5" />',
 # editing an element's outline: a shape with corner grips and an added one
 "edit-outline": '<path d="M5 19 4 6l12-2 4 13z" /> <rect x="2.5" y="4.5" width="3" height="3" /> <rect x="14.5" y="2.5" width="3" height="3" /> <rect x="18.5" y="15.5" width="3" height="3" /> <path d="m12 16.5 2 2-2 2-2-2z" />',
 # crop boxes (section boxes) of 3D views: a dashed box with grips
 "crop-box": '<path d="M4 7.5 12 3l8 4.5v9L12 21l-8-4.5z" stroke-dasharray="2.5 2" /> <path d="M4 7.5 12 12l8-4.5M12 12v9" /> <rect x="10.5" y="1.5" width="3" height="3" /> <rect x="10.5" y="19.5" width="3" height="3" />',
 # element types, curtain walls, wall openings and shafts
 "curtain-wall": '<rect x="3" y="3" width="18" height="18" /> <path d="M9 3v18M15 3v18M3 12h18" />',
 "wall-opening": '<rect x="3" y="3" width="18" height="18" rx="2" /> <rect x="8" y="8" width="8" height="8" stroke-dasharray="2 2" />',
 "shaft": '<path d="M3 8h18M3 16h18" /> <rect x="9" y="3" width="6" height="18" stroke-dasharray="2 2" />',
 "element-types": '<rect x="3" y="3" width="7" height="7" /> <rect x="14" y="3" width="7" height="7" /> <rect x="3" y="14" width="7" height="7" /> <path d="M14 17.5h7M17.5 14v7" />',
 # furniture and fixtures: the Object tool and the Catalogue
 "sofa": '<path d="M5 11V8a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2v3" /> <path d="M3 13a2 2 0 0 1 4 0v2h10v-2a2 2 0 0 1 4 0v4H3z" /> <path d="M5 17v2M19 17v2" />',
 # footings, and copying or moving elements to other levels
 "footing": '<path d="M9 3v11M15 3v11" /> <rect x="3" y="14" width="18" height="6" /> <path d="M1 10h4M19 10h4" stroke-dasharray="2 1.5" />',
 "copy-levels": '<path d="M2 7h11M2 19h11" stroke-dasharray="3 2" /> <rect x="15" y="2" width="6" height="5" /> <rect x="15" y="14" width="6" height="5" stroke-dasharray="2 1.5" /> <path d="M18 8.5v4M16.5 11l1.5 1.5 1.5-1.5" />',
 "move-level": '<path d="M2 7h11M2 19h11" stroke-dasharray="3 2" /> <rect x="15" y="14" width="6" height="5" /> <path d="M18 2v10M15.5 9.5 18 12l2.5-2.5" />',
}
out={}
for n in names:
    s=open(src+n+'.svg').read()
    s=re.sub(r'<!--.*?-->','',s,flags=re.S); inner=re.search(r'<svg[^>]*>(.*)</svg>', s, re.S).group(1)
    out[n]=re.sub(r'\s+',' ',inner).strip()
out.update(CUSTOM)
sys.stdout.write("// Icons from Lucide (https://lucide.dev), ISC License; see LICENSE-lucide.\n")
sys.stdout.write("window.ICONS = "+json.dumps(out, indent=0)+";\n")
