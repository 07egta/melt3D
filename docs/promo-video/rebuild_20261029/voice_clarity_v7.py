"""V7 voice-only clarity retake. Original visual edit and score remain the basis.
All speech is freshly synthesized, never sped-up mixed narration.
Display text is separate from pronunciation input. Voice: VOICEVOX:ずんだもん.
"""
from pathlib import Path
import dataclasses, io, json, os, subprocess, sys, urllib.request, wave
OUT=Path('v7-clarity-assets');OUT.mkdir(exist_ok=True)
# Times refer to the unchanged 170s V7 picture. The 4.8s tutorial introduction
# is inserted immediately before source time 126.0 by the final compositor.
LINES=[
('question_a',4.4,7.8,'3Dで、形を作りたい。','スリーディーで、形を作りたい。'),
('question_b',8.1,11.7,'でも、何から始めればいい？','でも、何から始めればいい？'),
('polygon',12.15,15.8,'ポリゴンは、点や面を動かして形を作る。','ポリゴンは、点や面を動かして形を作る。'),
('surface',16.0,19.8,'サーフェスは、曲面をつないで形を作る。','サーフェスは、曲面をつないで形を作る。'),
('soft',20.35,25.7,'丸や箱を組み合わせて、やわらかい形に。','丸や箱を組み合わせて、やわらかい形に。'),
('melt_name',26.25,28.5,'それが、Melt 3D。','それが、メルトスリーディー。'),
('melt_action',28.7,32.2,'形を重ねて、とけ具合を上げると、','形を重ねて、とけ具合を上げると、'),
('melt_result',32.35,35.8,'つなぎ目が、なめらかにつながるのだ。','つなぎ目が、なめらかにつながるのだ。'),
('presets_a',36.1,39.15,'こんなモデルを、すぐに使えるのだ。','こんなモデルを、すぐに使えるのだ。'),
('presets_b',39.3,42.5,'動物も、食べ物も、乗り物も。','動物も、食べ物も、乗り物も。'),
('presets_c',42.65,45.9,'プリセットから選んで、作り始めよう。','プリセットから選んで、作り始めよう。'),
('subtract_a',46.2,50.0,'穴を開けたいときは、別の形を重ねる。','穴を開けたいときは、別の形を重ねる。'),
('subtract_b',50.2,53.7,'「削る」に切り替えて、くり抜く。','削るに切り替えて、くり抜く。'),
('material_a',54.2,58.3,'色やつやを変えて、素材の印象を比べよう。','色やつやを変えて、素材の印象を比べよう。'),
('material_b',58.5,61.75,'金属感や、グラデーションも。','金属感や、グラデーションも。'),
('sketch_a',62.15,65.8,'平面に描いた輪郭を、上に押し出す。','平面に描いた輪郭を、上に押し出す。'),
('sketch_b',66.0,68.9,'高さを決めれば、立体になるのだ。','高さを決めれば、立体になるのだ。'),
('pipe_draw',69.15,72.0,'線を描いて、太さをつける。','線を描いて、太さをつける。'),
('pipe_whole',72.4,75.55,'全体を、太くすることも。','全体を、太くすることも。'),
('pipe_local',76.0,79.85,'選んだ点の周りだけ、太くすることもできる。','選んだ点の周りだけ、太くすることもできる。'),
('mirror',80.15,82.8,'ミラーで、左右対称に。','ミラーで、左右対称に。'),
('pattern',83.1,86.75,'パターンで、同じ形を並べるのだ。','パターンで、同じ形を並べるのだ。'),
('size_a',87.2,91.0,'サイズを変えながら、形を比べられる。','サイズを変えながら、形を比べられる。'),
('size_b',91.25,94.75,'バッグの幅も、数字で調整。','バッグの幅も、数字で調整。'),
('solid',95.15,98.65,'寸法を決めて作るなら、SolidCraft。','寸法を決めて作るなら、ソリッドクラフト。'),
('width',98.8,100.3,'幅は、60mm。','はばは、ろくじゅうミリ。'),
('depth',100.4,102.1,'奥行きは、40mm。','おくゆきは、よんじゅうミリ。'),
('height',102.2,103.9,'高さは、20mm。','たかさは、にじゅうミリ。'),
('anim_first',104.2,106.8,'最初のポーズを記録。','最初のポーズを記録。'),
('anim_last',107.0,109.8,'時間を進めて、別のポーズに。','時間を進めて、別のポーズに。'),
('anim_play',110.0,112.85,'再生すると、間がつながるのだ。','再生すると、あいだがつながるのだ。'),
('export_a',113.2,117.0,'できた形は、3Dプリント用に書き出し。','できた形は、スリーディープリント用に書き出し。'),
('export_b',117.2,121.65,'薄い部分や穴を調べて、印刷の準備へ。','薄い部分や穴を調べて、印刷の準備へ。'),
('program',122.15,125.8,'プログラムからも、形を作れるのだ。','プログラムからも、形を作れるのだ。'),
('frog_intro',126.0,130.8,'ここからは、カエルのペン立てを作ってみるのだ。','ここからは、カエルのペン立てを作ってみるのだ。'),
('frog_ball',126.25,128.85,'まずは、大きな丸を一つ。','まずは、大きな丸をひとつ。'),
('frog_flat',129.0,131.55,'少し平たくして、体にする。','少し平たくして、体にする。'),
('frog_copy',131.85,135.1,'小さな丸を置いて、反対側にも複製。','小さな丸を置いて、反対側にも複製。'),
('frog_melt',135.3,138.35,'とけ具合を変えて、丸さを調整。','とけ具合を変えて、丸さを調整。'),
('frog_back',138.55,140.65,'少し戻して、このくらい。','少し戻して、このくらい。'),
('frog_cut',140.85,144.2,'円柱を重ねて、「削る」に切り替える。','円柱を重ねて、削るに切り替える。'),
('frog_depth',144.35,147.0,'底を残して、穴の深さを決める。','底を残して、穴の深さを決める。'),
('frog_face',147.15,149.85,'緑にして、目と口をつける。','緑にして、目と口をつける。'),
('frog_face_done',150.05,152.05,'カエルらしくなったのだ。','カエルらしくなったのだ。'),
('frog_sketch',152.2,154.6,'輪郭を描いて、土台を作る。','輪郭を描いて、土台を作る。'),
('frog_extrude',154.75,156.9,'押し出す高さは、3mm。','押し出すたかさは、さんミリ。'),
('frog_finish',157.1,159.3,'光を整えて、完成。','光を整えて、完成。'),
('frog_check',159.5,163.4,'回して、穴や丸みも確認するのだ。','回して、穴や丸みも確認するのだ。'),
('outro_a',164.35,167.5,'思いついた形を、置いて、とかして。','思いついた形を、置いて、とかして。'),
('outro_b',167.7,169.8,'Melt 3D。','メルトスリーディー。'),
]
HEAD={'User-Agent':'Melt3D-voice-clarity'}
def get(url):
    h=dict(HEAD)
    if url.startswith('https://api.github.com/') and os.getenv('GH_TOKEN'):h['Authorization']='Bearer '+os.environ['GH_TOKEN']
    with urllib.request.urlopen(urllib.request.Request(url,headers=h),timeout=180) as r:return r.read()
def save(url,path):
    p=Path(path);p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(get(url));return p
release=json.loads(get('https://api.github.com/repos/VOICEVOX/voicevox_core/releases/tags/0.17.1'))
wa=next(a for a in release['assets'] if 'manylinux' in a['name'] and 'x86_64' in a['name'] and a['name'].endswith('.whl'))
wp=save(wa['browser_download_url'],wa['name']);subprocess.run([sys.executable,'-m','pip','install',str(wp)],check=True)
dl=next(a for a in release['assets'] if a['name']=='download-linux-x64');save(dl['browser_download_url'],'download');Path('download').chmod(0o755)
subprocess.run(['./download','--exclude','c-api','--models-version','0.17.0','--models-pattern','0.vvm','-o','vv-runtime'],input='y\n',text=True,check=True)
from voicevox_core.blocking import Onnxruntime,OpenJtalk,Synthesizer,VoiceModelFile
rt=Path('vv-runtime');lib=next((rt/'onnxruntime/lib').glob('libvoicevox_onnxruntime.so.*'))
syn=Synthesizer(Onnxruntime.load_once(filename=str(lib)),OpenJtalk(str(rt/'dict/open_jtalk_dic_utf_8-1.11')),acceleration_mode='CPU',cpu_num_threads=4)
with VoiceModelFile.open(str(rt/'models/vvms/0.vvm')) as vm:syn.load_voice_model(vm)
def asdict(q):return dataclasses.asdict(q) if dataclasses.is_dataclass(q) else vars(q)
voice=OUT/'voice';voice.mkdir(exist_ok=True);manifest=[]
for ident,start,end,text,reading in LINES:
    q=syn.create_audio_query(reading,3)
    q.speed_scale=1.12;q.intonation_scale=1.02;q.pitch_scale=0.;q.pre_phoneme_length=.09;q.post_phoneme_length=.14;q.volume_scale=1.;q.output_sampling_rate=24000;q.output_stereo=False
    # Keep punctuation pauses intact, but avoid excessively long default pauses.
    for ph in q.accent_phrases:
        if ph.pause_mora is not None:ph.pause_mora.vowel_length=min(.23,max(.13,ph.pause_mora.vowel_length))
    slot=end-start-.04
    for attempt in range(3):
        wav=syn.synthesis(q,3)
        with wave.open(io.BytesIO(wav)) as w:duration=w.getnframes()/w.getframerate()
        if duration<=slot or q.speed_scale>=1.21:break
        q.speed_scale=min(1.21,q.speed_scale*duration/slot+.004)
    (voice/(ident+'.wav')).write_bytes(wav)
    (voice/(ident+'.query.json')).write_text(json.dumps(asdict(q),ensure_ascii=False,indent=2))
    kana=' / '.join(''.join(m.text for m in p.moras) for p in q.accent_phrases)
    entry=dict(id=ident,source_start=start,source_end=end,text=text,reading=reading,kana=kana,duration=duration,speed=q.speed_scale,intro=(ident=='frog_intro'))
    manifest.append(entry);print(json.dumps(entry,ensure_ascii=False),flush=True)
(OUT/'narration.json').write_text(json.dumps(manifest,ensure_ascii=False,indent=2))
(OUT/'CREDIT.txt').write_text('VOICEVOX:ずんだもん\nOriginal V7 picture/score retained. Fresh pronunciation-aware narration.\n')
# Reusable runtime is a working artifact, NOT distributed in the user edit package.
import shutil
shutil.copy(wp,OUT/wp.name)
# Music separation fallback; this downloads the official pretrained weights only.
try:
    save('https://dl.fbaipublicfiles.com/demucs/hybrid_transformer/955717e8-8726e21a.th',OUT/'955717e8-8726e21a.th')
except Exception as e:print('Optional separation weights unavailable:',repr(e),flush=True)
