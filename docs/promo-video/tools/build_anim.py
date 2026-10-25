import sys,json,time
sys.path.insert(0,"/home/claude/video/rec")
from driver import App
a=App()
a.cmd({"cmd":"load","path":"/home/claude/video/char.meltcad"})
sc=a.cmd({"cmd":"scene.get"})
a.cmd({"cmd":"anim.set","fps":12,"length":36,"clay":True})
a.cmd({"cmd":"anim.key","frame":0})
targets=[(1,0),(1,1),(9,0),(9,1),(9,2),(9,3),(2,0)]   # face, neck, bean ribbon, blouse
orig={t:list(sc["groups"][t[0]]["shapes"][t[1]]["pos"]) for t in targets}
def setz(dz):
    for (g,s),p in orig.items():
        a.cmd({"cmd":"shape.set","group":g,"shape":s,"pos":[p[0],p[1],p[2]+dz]})
setz(9); a.cmd({"cmd":"anim.key","frame":12})
setz(0); a.cmd({"cmd":"anim.key","frame":24})
print(a.cmd({"cmd":"anim.get"}))
a.cmd({"cmd":"anim.render","path":"/home/claude/video/clips/07_anim_gif.gif","size":420})
a.cmd({"cmd":"save","path":"/home/claude/video/char_anim.meltcad"})
print("done")
