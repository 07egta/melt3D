"""English narration for the user's WAICY/overseas media kit.
Synthetic voice, not a recording of the student's own voice.
The first-person script is an editable draft for the student's presentation.
"""
from pathlib import Path
import json,numpy as np,soundfile as sf,torch
from kokoro import KPipeline
OUT=Path('submission-narration');OUT.mkdir(exist_ok=True)
SPEECH=[
('p01',20,"Hello. I'm the student developer of Melt 3D, from Japan. This open-source desktop modeller combines clay-like shaping, numerical dimensions, and an AI tool interface. The goal is simple: help students turn an idea into an editable design."),
('p02',19,"A 3D printer is only useful when you can design something to print. For a beginner, complex menus and geometry can interrupt that first idea. I want to make the starting point more approachable, without taking control away from the learner."),
('p03',23,"In Melt 3D, you place simple shapes, blend their joins, or subtract one from another. You can also sketch and extrude, set dimensions, and export files for 3D printing. Modelling runs locally. The optional AI connection is separate, and an online AI service may need internet access."),
('p04',31,"The AI layer uses the Model Context Protocol. An external language-model assistant interprets a request and calls two tools. Melt 3D run applies structured modelling commands. Melt 3D view returns an image of the result. This creates an editable starting point, not just a picture. The demo includes real protocol requests and returned previews. It does not pretend that a scripted test is an autonomous chat session."),
('p05',25,"My contribution is the Melt 3D modelling experience and its integration with an AI bridge. The project builds on open-source Solid Craft, rather than claiming that every CAD component was written from scratch. AI-assisted coding and iterative testing help development. The application validates commands, while the designer still reviews the shape and makes the final decisions."),
('p06',23,"Potential uses include classroom repair parts, accessible tools, and creative projects. These are intended uses, not claims of measured classroom adoption. My next evaluation is to compare how beginners complete a small modelling task, including the time, errors, and revisions they need. Print checks help review a model, but do not guarantee a safe or successful print."),
('p07',15,"The source code and demonstration are available through the links on this slide. The accompanying evidence includes editable scenes, tool responses, and screenshots. That makes the workflow inspectable and gives other learners a practical starting point."),
('p08',12,"Melt 3D is about making the first step easier, while keeping the design in human hands. Thank you. I welcome your questions."),
('mcp01',6.4,"This is Melt 3D's real MCP interface. Two tools: run commands, and inspect the scene."),
('mcp02',7.4,"These requests create editable shapes, adjust their blend, and return previews from the modeller."),
('mcp03',6.5,"An invalid reference is rejected. In this check, the scene stays unchanged."),
('bio',30,"Hi, I'm Taichiro Eguchi, a high-school student from Japan. I create 3D designs and develop Melt 3D, an open-source modeller with clay-like blending and an AI tool interface. I want to make it easier for beginners to turn their ideas into editable models, explore different shapes, and prepare their designs for 3D printing.")]
torch.set_num_threads(4);pipe=KPipeline(lang_code='a',repo_id='hexgrad/Kokoro-82M',device='cpu');rows=[];sr=24000
for ident,slot,text in SPEECH:
 reading=text.replace('Melt 3D run','Melt three dee run').replace('Melt 3D view','Melt three dee view').replace('Melt 3D','Melt three dee').replace('3D','three dee')
 speed=1.02
 for attempt in range(4):
  chunks=list(pipe(reading,voice='af_heart',speed=speed,split_pattern=r'\n+'))
  a=np.concatenate([np.asarray(x) for _,_,x in chunks]);active=np.flatnonzero(abs(a)>.003)
  if len(active):a=a[max(0,active[0]-int(.07*sr)):min(len(a),active[-1]+int(.16*sr))]
  dur=len(a)/sr
  if dur<slot-.55 or speed>=1.17:break
  speed=min(1.17,speed*dur/(slot-.55)+.004)
 sf.write(OUT/(ident+'.wav'),a,sr,subtype='PCM_24')
 row=dict(id=ident,slot=slot,text=text,reading=reading,speed=speed,duration=dur,phonemes=' | '.join(p for _,p,_ in chunks));rows.append(row);print(json.dumps(row),flush=True)
(OUT/'narration.json').write_text(json.dumps(rows,ensure_ascii=False,indent=2))
(OUT/'CREDITS.txt').write_text('Synthetic English voice: Kokoro-82M / af_heart. Not the student\'s recorded voice.\nScript and localization prepared with AI assistance.\n')
