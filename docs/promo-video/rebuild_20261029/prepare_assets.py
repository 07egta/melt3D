"""Prepare official VOICEVOX dialogue and original Melt3D assets for the film.
This is a video-only helper. No release or application files are changed.
"""
from pathlib import Path
import json, subprocess, sys, urllib.request, wave, dataclasses, os
ROOT=Path(__file__).resolve().parent
OUT=Path('video-assets'); OUT.mkdir(exist_ok=True)
LINES=[
('intent',4.3,10.8,'つくりたい形は、頭の中にある。もっと気軽に、試せたら。'),
('comparison',11.2,19.7,'ポリゴンは、頂点と面を整える。サーフェスは、曲面のつながりを調整する。'),
('soft',20.2,25.7,'丸や箱を、組み合わせる。つなぎ目まで、やわらかい形に。'),
('melt',26.2,33.7,'形を置いて、動かして、とかす。丸や箱から、なめらかな形を探せるのだ。'),
('gallery',34.2,45.6,'こんなものも作れるのだ。人、動物、街、家具。プリセットを選んで、自分の形に変えてみよう。'),
('subtract',46.2,53.7,'引いて作ることもできる。別の形を重ねて、削るに切り替えると、穴やくぼみになるのだ。'),
('material',54.2,61.7,'色、つや、金属感。形はそのまま、素材の印象を比べられるのだ。'),
('sketch',62.2,69.7,'輪郭を描いて、押し出す。高さを数字で決めれば、絵が厚みのある立体になる。'),
('pipe_a',70.2,73.9,'パイプは、なぞった線から作るのだ。'),
('pipe_b',74.1,81.7,'点を動かして曲げる。点ごとの太さも、全体の太さも変えられる。'),
('pattern_a',82.2,85.5,'ミラーで、片側から左右対称に。'),
('pattern_b',85.7,91.7,'パターンで、間隔や個数を決めて並べる。元の形を直せば、まとめて変わる。'),
('solid_a',92.2,96.2,'寸法を大切にしたい部分は、ソリッドクラフト。'),
('solid_b',96.4,101.7,'幅六十、奥行き四十、高さ二十ミリ。数値を決めて作るのだ。'),
('anim_a',102.2,106.6,'動かすときは、始めと終わりのポーズを記録。'),
('anim_b',106.8,111.7,'間は自動でつながる。メルトの量も、時間に合わせて変えられるのだ。'),
('export_a',112.2,116.5,'できた形は、エスティーエルやスリーエムエフで書き出し。'),
('export_b',116.7,120.7,'薄いところや穴を確認して、スリーディープリントにつなげるのだ。'),
('frog_intro',121.2,125.8,'丸三つから、カエルのペン立てを作るのだ。'),
('frog_body',126.2,131.7,'大きな丸を少し平たく。小さな丸を、左右に置く。'),
('frog_melt',132.2,136.7,'とけ具合を上げて、つなぎ目をなめらかに。'),
('frog_cut',137.2,143.7,'円柱を重ねて、削る。底を残して、ペンを入れる深さにする。'),
('frog_face',144.2,150.6,'緑にして、目と口を足す。ほら、表情が出てきたのだ。'),
('frog_base',151.2,155.7,'輪郭から土台を押し出す。高さは、三ミリ。'),
('frog_finish',156.2,161.7,'光とつやを整えて、完成。見る角度を変えて、形を確認するのだ。'),
('outro',163.0,169.2,'思いついた形を、置いて、とかして、形にする。メルトスリーディー。')]
HEAD={'User-Agent':'Melt3D-film-assets'}
def get(url):
    req=urllib.request.Request(url,headers=HEAD)
    with urllib.request.urlopen(req,timeout=120) as r:return r.read()
def save(url,path):
    path=Path(path); path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(get(url));return path
release=json.loads(get('https://api.github.com/repos/VOICEVOX/voicevox_core/releases/tags/0.17.1'))
wheel=next(a for a in release['assets'] if 'manylinux' in a['name'] and 'x86_64' in a['name'] and a['name'].endswith('.whl'))
wp=save(wheel['browser_download_url'],wheel['name'])
subprocess.run([sys.executable,'-m','pip','install',str(wp)],check=True)
dl=next(a for a in release['assets'] if a['name']=='download-linux-x64')
save(dl['browser_download_url'],'download');Path('download').chmod(0o755)
subprocess.run(['./download','--exclude','c-api','--models-version','0.17.0','--models-pattern','0.vvm','-o','vv-runtime'],check=True)
from voicevox_core.blocking import Onnxruntime,OpenJtalk,Synthesizer,VoiceModelFile
rt=Path('vv-runtime')
lib=next((rt/'onnxruntime/lib').glob('libvoicevox_onnxruntime.so.*'))
syn=Synthesizer(Onnxruntime.load_once(filename=str(lib)),OpenJtalk(str(rt/'dict/open_jtalk_dic_utf_8-1.11')),acceleration_mode='CPU',cpu_num_threads=4)
with VoiceModelFile.open(str(rt/'models/vvms/0.vvm')) as model:syn.load_voice_model(model)
voice=OUT/'voice';voice.mkdir(exist_ok=True)
manifest=[]
for ident,start,end,text in LINES:
    q=syn.create_audio_query(text,3)
    q.speed_scale=1.28;q.intonation_scale=1.08;q.pre_phoneme_length=.08;q.post_phoneme_length=.12
    data=syn.synthesis(q,3);path=voice/(ident+'.wav');path.write_bytes(data)
    with wave.open(str(path)) as f:dur=f.getnframes()/f.getframerate()
    record={'id':ident,'start':start,'end':end,'text':text,'duration':dur,'file':str(path.relative_to(OUT))}
    manifest.append(record);print(record,flush=True)
(OUT/'narration.json').write_text(json.dumps(manifest,ensure_ascii=False,indent=2))
(OUT/'CREDIT.txt').write_text('VOICEVOX:ずんだもん\nOfficial VOICEVOX CORE 0.17.1 / voicevox_vvm 0.17.0.\n')
# Download the actual library source used by the original film build, not invented substitutes.
base='https://raw.githubusercontent.com/07egta/melt3D/85d821532928e0efa8f19503da564bf3e5719468/'
for name in ['people','animals','furniture','food','vehicles','buildings','nature','town','tools_a','tools_b']:
    save(base+'apps/meltcad/library/'+name+'.json',OUT/'library'/(name+'.json'))
for name in ['sdf.rs','curve.rs','cad.rs','prims.rs','cpu_render.rs','model.rs','library.rs','automation.rs']:
    save(base+'apps/meltcad/src/'+name,OUT/'source_reference'/name)
for name in ['compose.py','take_anim.py','take_melt.py','take_ops.py','take_sketch.py','take_solid.py','driver.py']:
    save('https://raw.githubusercontent.com/07egta/melt3D/promo-video/docs/promo-video/tools/'+name,OUT/'original_tools'/name)
(OUT/'SOURCE.txt').write_text('Original app reference commit: 85d821532928e0efa8f19503da564bf3e5719468\nRecording scripts: branch promo-video\n')
