"""Capture the actual Melt3D UI and renderer; editing-only fixture, not application code.
Uses the same offscreen GPU capture route as the recovered 2026-10-26 film sources.
"""
from pathlib import Path
import json, os, subprocess, math, copy
APP=Path('native-app').resolve(); OUT=Path('native-frames').resolve(); OUT.mkdir(exist_ok=True)
SRC=APP/'apps/meltcad/src'
# Only change the temporary checkout, not the repository's application branch.
p=SRC/'view.rs';s=p.read_text();s=s.replace('whole: clean,','whole: clean || (cfg!(test) && std::env::var_os("MELT_VIDEO_WHOLE").is_some()),');p.write_text(s)
p=SRC/'app_preview_gpu.rs';s=p.read_text().replace('(size.x * ppp).ceil()','(size.x * ppp).round()').replace('(size.y * ppp).ceil()','(size.y * ppp).round()');p.write_text(s)
rust=r'''
#[test]
#[ignore]
fn source_film_capture() {
 use serde_json::Value;
 let shots:Vec<Value>=serde_json::from_str(&std::fs::read_to_string(std::env::var("FILM_MANIFEST").unwrap()).unwrap()).unwrap();
 let ppp=1.5;let size=egui::vec2(1280.0,720.0);let(w,h)=(1920usize,1080usize);
 let ctx=egui::Context::default();crate::fonts::setup(&ctx);style::apply(&ctx);ctx.set_pixels_per_point(ppp);
 let mut gpu=app_preview_gpu::GpuPreview::new().expect("native GPU renderer");
 let mut app=App::new(false,None);app.attach_gpu(Some(gpu.render_state.clone()),&ctx,false);
 let start=std::time::Instant::now();
 loop {match app.gpu.status.snapshot().state{crate::gpu_status::State::Ready{..}=>break,crate::gpu_status::State::Failed{message}=>panic!("GPU: {message}"),_=>{assert!(start.elapsed().as_secs()<240);std::thread::sleep(std::time::Duration::from_millis(100));}}}
 let mut clock=5.0;
 for shot in shots {
  app.sk=Default::default();app.solid_mode=false;
  app.scene=Scene::from_json(&shot["scene"].to_string()).expect("scene fixture");app.path=None;app.doc_title=shot["name"].as_str().unwrap().to_string();
  app.committed_sync();app.sel=match shot["select"].as_array(){Some(a) if a.len()==2=>Some(Pick::Shape(a[0].as_u64().unwrap() as usize,a[1].as_u64().unwrap() as usize)),Some(a) if a.len()==1=>Some(Pick::Group(a[0].as_u64().unwrap() as usize)),_=>None};
  app.multi.clear();app.hover=None;app.fly=None;app.lib_open=false;app.an.open=false;app.an.playing=false;app.show_keys=false;app.perf.hud=false;app.perf.quality=crate::perf::Quality::Fine;
  app.tab=match shot["tab"].as_str(){Some("melt")=>Tab::Melt,Some("material")=>Tab::Material,_=>Tab::Shape};
  app.cam.ortho=true;app.cam.yaw=shot["yaw"].as_f64().unwrap_or(0.0) as f32;app.cam.pitch=shot["pitch"].as_f64().unwrap_or(0.18) as f32;app.frame_now();app.fly=None;
  if let Some(v)=shot["dist"].as_f64(){app.cam.dist=v as f32;}
  if let Some(a)=shot["target"].as_array(){app.cam.target=[a[0].as_f64().unwrap() as f32,a[1].as_f64().unwrap() as f32,a[2].as_f64().unwrap() as f32];}
  app.perf.activity=Default::default();app.perf.last_cam=crate::perf::cam_key(&app.cam);
  let mut composed=egui::FullOutput::default();
  for _ in 0..8 {clock+=0.5;let input=egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,size)),time:Some(clock),..Default::default()};composed.append(ctx.run_ui(input,|ui|app.frame_body(ui)));}
  let pixels=gpu.render(&ctx,&composed,size,ppp);let out=shot["out"].as_str().unwrap();write_preview(pixels,w,h,out);
  let r=app.view_rect;std::fs::write(format!("{out}.json"),serde_json::json!({"viewport":[r.min.x*ppp,r.min.y*ppp,r.max.x*ppp,r.max.y*ppp],"name":shot["name"],"camera":{"yaw":app.cam.yaw,"pitch":app.cam.pitch,"dist":app.cam.dist,"target":app.cam.target}}).to_string()).unwrap();
  eprintln!("FILM_CAPTURE {out}");
 }
}
'''
with (SRC/'app_preview.rs').open('a') as f:f.write(rust)
def shape(kind='Sphere',name='Shape',pos=(0,0,20),size=(20,1,1),color=(.9,.3,.48),**kw):
 s=dict(name=name,kind=kind,pos=list(pos),size=list(size),rot=[0,0,0],scale=[1,1,1],round=0,op='Add',blend=6,visible=True,outline=[],outline2=[],path=[],params=[0,0,0,0],shell=0,material=dict(color=list(color),roughness=.3,metallic=.06,coat=.25));s.update(kw);return s
ENV=dict(sky_top=[.8,.86,.95],sky_horizon=[.95,.94,.9],sky_bottom=[.5,.5,.5],backdrop=[.025,.028,.034],exposure=1.0,grid=False,floor=True,shadow=.35,ambient=.85,lights=[dict(yaw=-35,pitch=55,power=1.25,color=[1,.97,.95]),dict(yaw=75,pitch=25,power=.65,color=[.88,.93,1]),dict(yaw=170,pitch=60,power=.6,color=[1,.95,.9])])
def scene(shapes,name='Model',pattern=None,melt=1):
 g=dict(name=name,op='Add',blend=0,visible=True,shapes=shapes)
 if pattern:g['pattern']=pattern
 return dict(groups=[g],melt=dict(mode='Smooth',amount=melt,drip=0,ripple=1),env=ENV)
shots=[]
def shot(name,sc,select=None,tab='shape',**kw):
 d=dict(name=name,scene=sc,select=select,tab=tab,out=str(OUT/(name+'.png')));d.update(kw);shots.append(d)
for i in range(25):
 u=i/24
 s=[shape('Box','Box',(-18,0,20),(32,32,38),round=5,blend=0),shape('Torus','Loop',(12,0,33),(21,7,1),rot=[90,0,0],blend=15)]
 shot('melt_%02d'%i,scene(s,melt=u*2.5),[0,1],'melt',dist=160,yaw=.32,pitch=.32,target=[0,0,25])
for i in range(25):
 u=i/24;nodes=[[-36,0,7,4],[-28,0,32,4],[0,0,47+u*12,4],[28,0,32,4],[36,0,7,4]]
 nodes[2][3]=4+u*5
 s=shape('Curve','Curve',pos=(0,0,0),size=(1,1,1),path=nodes,params=[0,.7,0,1],color=(.65,.86,.10),blend=0)
 shot('pipe_%02d'%i,scene([s]),[0,0],dist=175,yaw=.12,pitch=.3,target=[0,0,30])
for i in range(16):
 u=i/15;s=shape('Curve','Curve',pos=(0,0,0),size=(1,1,1),path=[[-36,0,7,4],[-28,0,32,4],[0,0,59,9],[28,0,32,4],[36,0,7,4]],params=[0,.7,0,1+u*.55],color=(.65,.86,.10),blend=0)
 shot('pipe_scale_%02d'%i,scene([s]),[0,0],dist=175,yaw=.12,pitch=.3,target=[0,0,30])
for i in range(5):
 s=[shape('Sphere','Bead',pos=(-44,0,12),size=(9,1,1),color=(.48,.4,.9),blend=0)]
 shot('pattern_%02d'%i,scene(s,pattern={'Rect':{'counts':[i+1,2,1],'spacing':[22,26,0]}}),[0],dist=190,yaw=.35,pitch=.7,target=[0,13,12])
for i in range(2):
 s=[shape('Capsule','Side',pos=(22,0,22),size=(8,25,1),rot=[0,25,0],color=(.48,.4,.9),blend=0)]
 shot('mirror_%02d'%i,scene(s,pattern={'Mirror':{'axes':[bool(i),False,False],'centre':False}}),[0],dist=150,yaw=.15,pitch=.25,target=[0,0,22])
# Authentic built-in models. No repetitions or substitutes.
choices={'people':[0,1,4],'animals':[0,1,2,3],'town':[0,1,3],'vehicles':[0,1,2],'furniture':[0,1,2,7],'food':[0,2,5],'nature':[0,3,6]}
for cat,idxs in choices.items():
 data=json.loads((APP/'apps/meltcad/library'/f'{cat}.json').read_text())
 for idx in idxs:
  if idx>=len(data['items']):continue
  it=data['items'][idx];sc=dict(groups=[it['group']],melt=dict(mode='Smooth',amount=1,drip=0,ripple=1),env={**ENV,'floor':False})
  shot('library_%s_%02d'%(cat,idx),sc,yaw=0,pitch=.10 if cat in ['people','animals'] else .22)
(OUT/'manifest.json').write_text(json.dumps(shots,ensure_ascii=False,indent=2))
print('Captured plans:',len(shots),flush=True)
env={**os.environ,'FILM_MANIFEST':str(OUT/'manifest.json'),'MELT_VIDEO_WHOLE':'1','WGPU_BACKEND':'vulkan','XDG_RUNTIME_DIR':'/tmp'}
subprocess.run(['cargo','test','--profile','ci','--locked','-p','meltcad','source_film_capture','--','--ignored','--nocapture','--test-threads=1'],cwd=APP,env=env,check=True)
