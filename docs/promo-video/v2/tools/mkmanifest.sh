cd /home/claude/video2
python3 - <<'P'
import glob,json,subprocess,os,shutil
m={}
for f in sorted(glob.glob('clips/*.mp4')):
    n=os.path.basename(f)[:-4]
    d=float(subprocess.check_output(['ffprobe','-v','error','-show_entries','format=duration','-of','csv=p=0',f]))
    m[n]={'dur':round(d,2)}
    mk=f.replace('.mp4','.marks.json')
    if os.path.exists(mk): m[n]['marks']=json.load(open(mk))
    shutil.copy(f,f'rm/public/clips/{n}.mp4')
json.dump(m,open('rm/public/manifest.json','w'))
print(m)
P
