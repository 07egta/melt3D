"""English localization of the approved V7 Clear Voice film.
Video-only tooling; no changes to application or releases.
Natural English display text is also the spoken script.
"""
from pathlib import Path
import json, os, math
import numpy as np
import soundfile as sf
import torch
from kokoro import KPipeline
ROOT=Path('english-voice-assets'); ROOT.mkdir(exist_ok=True)
D=128/30  # approved tutorial introduction inserted at source time 126
LINES=[
('question_a',4.40,7.80,'An idea for a 3D model.'),
('question_b',8.10,11.70,'But where do you start?'),
('polygon',12.15,15.80,'With polygons, you shape vertices and faces.'),
('surface',16.00,19.80,'With surfaces, you join curved patches.'),
('soft',20.35,25.70,'Or start simple. Combine spheres and boxes.'),
('melt_name',26.25,28.50,'Meet Melt 3D.'),
('melt_action',28.70,32.20,'Overlap shapes. Turn up the blend.'),
('melt_result',32.35,35.80,'Hard edges flow into smooth connections.'),
('presets_a',36.10,39.15,'Start with a ready-made model.'),
('presets_b',39.30,42.50,'Animals. Food. Vehicles.'),
('presets_c',42.65,45.90,'Pick a preset, and make it your own.'),
('subtract_a',46.20,50.00,'Need an opening? Overlap another shape.'),
('subtract_b',50.20,53.70,'Switch to Subtract. Cut it out.'),
('material_a',54.20,58.30,'Explore color, gloss, and metallic finishes.'),
('material_b',58.50,61.75,'Same shape. A whole new feel.'),
('sketch_a',62.15,65.80,'Sketch an outline, then extrude it.'),
('sketch_b',66.00,68.90,'Set the height. Make it three-dimensional.'),
('pipe_draw',69.15,72.00,'Draw a path. Give it thickness.'),
('pipe_whole',72.40,75.55,'Widen the entire tube.'),
('pipe_local',76.00,79.85,'Or adjust the thickness at individual points.'),
('mirror',80.15,82.80,'Mirror it for perfect symmetry.'),
('pattern',83.10,86.75,'Use patterns to repeat a shape.'),
('size_a',87.20,91.00,'Change the dimensions. Explore the design.'),
('size_b',91.25,94.75,'Fine-tune the width of a bag.'),
('solid',95.15,98.65,'For precise dimensions, use SolidCraft.'),
('width',98.80,100.30,'Sixty wide.'),
('depth',100.40,102.10,'Forty deep.'),
('height',102.20,103.90,'Twenty high. In millimeters.'),
('anim_first',104.20,106.80,'Save the first pose.'),
('anim_last',107.00,109.80,'Move forward. Save another.'),
('anim_play',110.00,112.85,'Press play. Watch them connect.'),
('export_a',113.20,117.00,'Export your model for 3D printing.'),
('export_b',117.20,121.65,'Check thin walls and openings before you print.'),
('program',122.15,125.80,'You can create shapes with code, too.'),
('frog_intro',126.30,130.10,"Now, let's make a frog-shaped pen holder."),
('frog_ball',126.25,128.85,'Start with one large sphere.'),
('frog_flat',129.00,131.55,'Flatten it a little for the body.'),
('frog_copy',131.85,135.10,'Add a smaller sphere. Duplicate it.'),
('frog_melt',135.30,138.35,'Adjust the blend to soften the joins.'),
('frog_back',138.55,140.65,'A little less. Just right.'),
('frog_cut',140.85,144.20,'Add a cylinder. Switch to Subtract.'),
('frog_depth',144.35,147.00,'Set the depth. Keep a solid bottom.'),
('frog_face',147.15,149.85,'Go green. Add eyes and a mouth.'),
('frog_face_done',150.05,152.05,"There's our frog."),
('frog_sketch',152.20,154.60,'Sketch an outline for the base.'),
('frog_extrude',154.75,156.90,'Extrude it three millimeters.'),
('frog_finish',157.10,159.30,'Light it. Finished.'),
('frog_check',159.50,163.40,'Rotate it. Check the opening and the curves.'),
('outro_a',164.35,167.50,'Place it. Blend it. Make it yours.'),
('outro_b',167.70,169.80,'Melt 3D.')]
torch.set_num_threads(4)
pipeline=KPipeline(lang_code='a', repo_id='hexgrad/Kokoro-82M',device='cpu')
voice='af_heart'; sr=24000; manifest=[]
for ident,ss,se,text in LINES:
 start=ss if ss<126 or ident=='frog_intro' else ss+D
 end=se if ss<126 or ident=='frog_intro' else se+D
 # Keep narration safely inside the corresponding shot, without clipping a word.
 budget=end-start-.10
 speed=1.03 if ident not in ['width','depth','height'] else 1.07
 reading=text.replace('Melt 3D','Melt three dee').replace('SolidCraft','Solid Craft').replace('3D','three dee')
 if ident=='height':reading='Twenty high.';text='Twenty high.'
 for attempt in range(3):
  outputs=list(pipeline(reading,voice=voice,speed=speed,split_pattern=r'\n+'))
  audio=np.concatenate([a.numpy() if hasattr(a,'numpy') else np.array(a) for _,_,a in outputs])
  # Trim only excess silence, never speech. Maintain breathing space.
  active=np.flatnonzero(np.abs(audio)>.004)
  if len(active):audio=audio[max(0,active[0]-int(.045*sr)):min(len(audio),active[-1]+int(.10*sr))]
  duration=len(audio)/sr
  if duration<=budget or speed>=1.17:break
  speed=min(1.17,speed*duration/budget+.008)
 if duration>budget+.12:
  print('FIT WARNING',ident,duration,budget,flush=True)
 sf.write(ROOT/(ident+'.wav'),audio,sr,subtype='PCM_24')
 row=dict(id=ident,start=start,end=end,text=text,reading=reading,duration=duration,speed=speed,voice=voice,phonemes=' | '.join(p for _,p,_ in outputs))
 manifest.append(row);print(json.dumps(row),flush=True)
(ROOT/'narration_en.json').write_text(json.dumps(manifest,indent=2))
(ROOT/'CREDITS.txt').write_text('English narration: Kokoro-82M, af_heart synthetic voice.\nModel: hexgrad/Kokoro-82M (Apache-2.0).\nEnglish script and timing created for Melt3D.\n')
