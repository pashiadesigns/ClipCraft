"""Generate ClipCraft's original starter stamp artwork (Pillow, optional asset tool)."""
from pathlib import Path
from PIL import Image, ImageDraw
ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/'crates/core/assets/icon_library'
S=4
GREEN='#198754'; RED='#d5373d'; GOLD='#f4bc34'; BLUE='#198fbd'; INK='#252729'

def stamp(name, symbol, circle=None, ink=INK):
    im=Image.new('RGBA',(128*S,128*S));d=ImageDraw.Draw(im)
    def line(points, fill, width):
        points=[(round(x*S),round(y*S)) for x,y in points]
        d.line(points,fill=fill,width=width*S,joint='curve')
        r=width*S/2
        for x,y in points:d.ellipse((x-r,y-r,x+r,y+r),fill=fill)
    def dot(x,y,r,fill):d.ellipse(((x-r)*S,(y-r)*S,(x+r)*S,(y+r)*S),fill=fill)
    if circle:d.ellipse((6*S,6*S,122*S,122*S),fill=circle)
    if symbol=='check':line([(30,64),(53,87),(98,39)],ink,12)
    if symbol=='x':
        line([(39,39),(89,89)],ink,12);line([(89,39),(39,89)],ink,12)
    if symbol=='caution':
        d.regular_polygon((64*S,68*S,59*S),n_sides=3,rotation=0,fill=GOLD,outline=INK,width=4*S)
        line([(64,48),(64,68)],INK,9);dot(64,84,5,INK)
    if symbol=='exclamation':line([(64,31),(64,75)],ink,11);dot(64,96,6,ink)
    if symbol=='info':dot(64,34,6,ink);line([(64,55),(64,95)],ink,10)
    if symbol=='question':
        # Original rounded question-mark centerline.
        d.arc((40*S,24*S,88*S,64*S),180,450,fill=ink,width=9*S)
        dot(44,44,4.5,ink);line([(64,60),(64,79)],ink,9)
        dot(64,98,5,ink)
    OUT.mkdir(parents=True,exist_ok=True)
    im.resize((128,128),Image.Resampling.LANCZOS).save(OUT/(name+'.png'),optimize=True)

if __name__=='__main__':
    stamp('check-green','check',ink=GREEN)
    stamp('check-circle','check',circle=GREEN,ink='white')
    stamp('cross-red','x',ink=RED)
    stamp('cross-circle','x',circle=RED,ink='white')
    stamp('caution-yellow','caution')
    stamp('exclamation','exclamation')
    stamp('exclamation-circle','exclamation',circle=RED,ink='white')
    stamp('question','question',circle=BLUE,ink='white')
    stamp('information','info',circle=BLUE,ink='white')
    print('Generated 9 original 128px transparent starter icons.')
