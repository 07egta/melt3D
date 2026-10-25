import json, numpy as np, subprocess, wave, math
SR=48000; BPM=128; BEAT=60/BPM; GRID=BEAT*2
S=json.load(open('sections13.json'))
def rd(f):
    w=wave.open(f); n=w.getnframes(); a=np.frombuffer(w.readframes(n),dtype=np.int16).astype(np.float32)/32768
    if w.getnchannels()==2: a=a.reshape(-1,2).mean(1)
    return a
def plan(gapmin):
    t=GRID*4; sc=[]
    for s in S:
        st=math.ceil(t/GRID-1e-6)*GRID; sc.append(round(st,4)); t=st+s['dur']+gapmin
    return sc,t
lo,hi=0.0,6.0
for _ in range(30):
    mid=(lo+hi)/2; sc,t=plan(mid)
    if t-mid+8*BEAT*0+GRID*8 > 170: hi=mid
    else: lo=mid
sched,t=plan(lo)
end_voice=sched[-1]+S[-1]['dur']
total=math.ceil((170)/BEAT)*BEAT
N=int(total*SR)
voice=np.zeros(N,np.float32)
for s,st in zip(S,sched):
    a=rd(s['wav']); i=int(st*SR); voice[i:i+len(a)]+=a[:N-i]
# music
rng=np.random.default_rng(1)
mus=np.zeros(N,np.float32)
def put(sig,t0,g=1.0):
    i=int(t0*SR); j=min(N,i+len(sig)); 
    if i<N: mus[i:j]+=sig[:j-i]*g
tk=np.arange(int(0.25*SR))/SR
kick=np.sin(2*np.pi*(55+120*np.exp(-tk*30))*tk)*np.exp(-tk*14)
th=np.arange(int(0.05*SR))/SR; hat=rng.standard_normal(len(th))*np.exp(-th*90)*0.35
tc=np.arange(int(0.18*SR))/SR; clap=rng.standard_normal(len(tc))*np.exp(-tc*22)*0.5
chords=[[261.6,329.6,392.0],[196.0,246.9,392.0],[220.0,261.6,329.6],[174.6,220.0,349.2]]
bass=[65.4,49.0,55.0,43.65]
nb=int(total/BEAT)
for b in range(nb):
    tt=b*BEAT; bar=(b//4)%4
    if b%1==0: put(kick,tt,0.9)
    put(hat,tt+BEAT/2,0.8)
    if b%2==1: put(clap,tt,0.7)
    # bass 8th notes
    for h in (0,1):
        f=bass[bar]*(2 if h else 1)
        n=np.arange(int(BEAT/2*SR))/SR; put(np.sin(2*np.pi*f*n)*np.exp(-n*5)*0.5,tt+h*BEAT/2,0.8)
    # pluck arpeggio 16ths
    for q in range(4):
        f=chords[bar][ (b*4+q)%3 ]*2
        n=np.arange(int(0.22*SR))/SR; pl=(np.sin(2*np.pi*f*n)+0.4*np.sin(4*np.pi*f*n))*np.exp(-n*14)
        put(pl,tt+q*BEAT/4,0.16)
mus/=max(1e-6,np.abs(mus).max())
# sidechain duck under voice
env=np.abs(voice); k=int(0.05*SR); env=np.convolve(env,np.ones(k)/k,'same'); duck=1-0.55*np.clip(env*6,0,1)
mix=voice*1.0+mus*0.32*duck
mix=np.tanh(mix*1.1)
pcm=(mix*32767).astype(np.int16)
with wave.open('out/mix.wav','wb') as w: w.setnchannels(1); w.setsampwidth(2); w.setframerate(SR); w.writeframes(pcm.tobytes())
# amplitude per frame (voice only) 30fps
fps=30; nf=int(total*fps)
amp=[float(np.sqrt(np.mean(voice[int(i/fps*SR):int((i+1)/fps*SR)]**2))) for i in range(nf)]
m=max(amp); amp=[round(a/m,3) for a in amp]
json.dump({"fps":fps,"frames":nf,"beat":BEAT,"total":total,"sections":[{"start":st,"dur":s['dur'],"head":s['head'],"sub":s['sub'],"sec":s['sec']} for s,st in zip(S,sched)],"amp":amp},open('rm/public/timeline.json','w'),ensure_ascii=False)
print(sched[:5], total, nf)
