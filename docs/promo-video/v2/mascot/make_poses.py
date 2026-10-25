import json,copy,math,subprocess,os,sys
base=json.load(open("base.meltcad"))
def sh(s,g,i): return s["groups"][g]["shapes"][i]
def move(s,g,i,dx=0,dy=0,dz=0):
    p=sh(s,g,i)["pos"]; p[0]+=dx; p[1]+=dy; p[2]+=dz
def rot_head(s,deg):
    a=math.radians(deg); cx,cz=0.0,84.0
    targets=[(0,i) for i in range(len(s["groups"][0]["shapes"]))]+[(1,0)]+[(9,i) for i in range(4)]+[(10,i) for i in range(5)]+[(11,i) for i in range(2)]+[(8,i) for i in range(5) if False]
    for g,i in targets:
        p=sh(s,g,i)["pos"]; x,z=p[0]-cx,p[2]-cz
        p[0]=cx+x*math.cos(a)-z*math.sin(a); p[2]=cz+x*math.sin(a)+z*math.cos(a)
def mouth(s,open_):
    m=sh(s,10,4)
    if open_: m["scale"]=[m["scale"][0]*1.5,m["scale"][1]*1.5,m["scale"][2]*2.6]
def blink(s):
    for i in (0,1,2,3):
        sc=sh(s,10,i)["scale"]; sc[2]*=0.14
def wave(s):   # character's left hand (x<0) up
    move(s,3,0,-4,0,10); move(s,3,1,-9,-2,32); move(s,3,2,-8,-3,33)
    move(s,2,2,-3,0,6)
def cheer(s):
    wave(s); move(s,3,3,4,0,10); move(s,3,4,9,-2,32); move(s,3,5,8,-3,33); move(s,2,4,3,0,6)
def bounce(s,dz):
    for g in s["groups"]:
        for x in g["shapes"]: x["pos"][2]+=dz
poses={}
def P(name,*ops):
    s=copy.deepcopy(base)
    for f,a in ops: f(s,*a)
    poses[name]=s
P("idle_c")
P("idle_o",(mouth,(True,)))
P("blink",(blink,()))
P("wave_c",(wave,()),(rot_head,(-5,)))
P("wave_o",(wave,()),(mouth,(True,)),(rot_head,(-5,)))
P("cheer_c",(cheer,()),(bounce,(6,)))
P("cheer_o",(cheer,()),(mouth,(True,)),(bounce,(6,)))
P("tiltL_c",(rot_head,(9,)))
P("tiltL_o",(rot_head,(9,)),(mouth,(True,)))
P("tiltR_c",(rot_head,(-9,)))
P("tiltR_o",(rot_head,(-9,)),(mouth,(True,)))
P("hop_o",(bounce,(9,)),(mouth,(True,)))
for n,s in poses.items(): json.dump(s,open(f"{n}.meltcad","w"))
print(list(poses))
