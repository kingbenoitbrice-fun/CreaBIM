"""Make icons.js from the lucide-static icons folder: make_icons.py ICONS_DIR > icons.js"""
import re, sys, json
src=sys.argv[1].rstrip('/')+'/'
names="""mouse-pointer-2 brick-wall door-open app-window layers layers-2 box scissors-line-dashed building building-2 ruler tag pen-tool slash undo-2 redo-2 move rotate-cw scaling copy-plus copy scissors clipboard-paste trash-2 eye-off focus scan-eye eye folder chevron-right chevron-down search file magnet grid-3x3 square-dashed crosshair check arrow-left layers-3 columns-2 scan sticky-note frame panel-left hash axis-3d circle-dot move-3d spline file-plus folder-open save download upload link sheet table-2 refresh-cw cuboid house map-pin layout-dashboard square-dashed-mouse-pointer git-commit-horizontal ellipsis sun moon panel-right panel-bottom maximize-2 minus diamond palette map file-text drafting-compass radius diameter message-square-text arrow-up-right""".split()
out={}
for n in names:
    s=open(src+n+'.svg').read()
    s=re.sub(r'<!--.*?-->','',s,flags=re.S); inner=re.search(r'<svg[^>]*>(.*)</svg>', s, re.S).group(1)
    out[n]=re.sub(r'\s+',' ',inner).strip()
sys.stdout.write("// Icons from Lucide (https://lucide.dev), ISC License; see LICENSE-lucide.\n")
sys.stdout.write("window.ICONS = "+json.dumps(out, indent=0)+";\n")
