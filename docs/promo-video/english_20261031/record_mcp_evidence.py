"""Run a real, bounded MCP session against the unchanged Melt3D binary.
This is an AI-authored tool client, NOT a simulated Claude chat. Raw responses,
images and verification assertions are retained for the submission evidence.
"""
from pathlib import Path
import base64,hashlib,json,os,subprocess,sys,time
OUT=Path('mcp-evidence');OUT.mkdir(exist_ok=True)
BIN=Path(sys.argv[1]).resolve()
err=(OUT/'server_stderr.txt').open('w')
p=subprocess.Popen([str(BIN),'--mcp'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=err,text=True,bufsize=1)
started=time.monotonic();seq=0;records=[];readable=[];stage=[]
def rpc(method,params):
 global seq
 seq+=1
 q={'jsonrpc':'2.0','id':seq,'method':method,'params':params}
 p.stdin.write(json.dumps(q)+'\n');p.stdin.flush()
 line=p.stdout.readline()
 if not line:raise RuntimeError('MCP server closed before replying')
 a=json.loads(line)
 records.append({'elapsed':round(time.monotonic()-started,3),'request':q,'response':a})
 if 'error' in a:raise RuntimeError(a['error'])
 return a['result']
def run(label,commands,allow_error=False):
 a=rpc('tools/call',{'name':'melt3d_run','arguments':{'commands':commands}})
 rows=json.loads(a['content'][0]['text'])
 readable.append({'stage':label,'commands':commands,'results':rows,'elapsed':round(time.monotonic()-started,3)})
 if not allow_error:assert not a.get('isError'),rows
 return rows
def view(name):
 a=rpc('tools/call',{'name':'melt3d_view','arguments':{'size':500}})
 im=next(v for v in a['content'] if v['type']=='image')
 b=base64.b64decode(im['data']);(OUT/(name+'.png')).write_bytes(b)
 stage.append({'image':name+'.png','sha256':hashlib.sha256(b).hexdigest(),'elapsed':round(time.monotonic()-started,3)})
 return name
hello=rpc('initialize',{'protocolVersion':'2024-11-05','capabilities':{},'clientInfo':{'name':'Melt3D-Submission-Evidence-Client','version':'1.0'}})
tools=rpc('tools/list',{})
assert {'melt3d_run','melt3d_view'}<=set(t['name'] for t in tools['tools'])
run('New scene',[{'cmd':'scene.new'}])
run('Create editable primitives',[
 {'cmd':'shape.add','kind':'Box','name':'Rounded base','pos':[-10,0,20],'size':[38,32,40],'round':5,'blend':0,'color':[.85,.24,.47]},
 {'cmd':'shape.add','kind':'Torus','name':'Loop','pos':[14,0,40],'size':[23,7,1],'rot':[90,0,0],'blend':16,'color':[.85,.24,.47]},
 {'cmd':'melt.set','amount':0}])
view('01_separate_shapes')
run('Blend the joins',[{'cmd':'melt.set','amount':1.4}]);view('02_melted')
run('Refine with exact coordinates',[{'cmd':'shape.set','group':0,'shape':1,'pos':[9,0,36]}]);view('03_refined')
before=run('Read scene before rejected edit',[{'cmd':'scene.get'}])[0]['result']
fail=run('Reject an invalid reference',[{'cmd':'shape.set','group':999,'shape':999,'pos':[0,0,0]}],True)
after=run('Verify scene is unchanged',[{'cmd':'scene.get'}])[0]['result']
assert before==after and fail[0]['ok'] is False
run('Save the editable scene',[{'cmd':'save','path':str((OUT/'MCP_editable_model.meltcad').resolve())}])
run('Sketch to solid',[
 {'cmd':'scene.new'},
 {'cmd':'sketch.create','name':'Base profile','points':[[-30,-20],[30,-20],[30,20],[-30,20]]},
 {'cmd':'solid.extrude','sketch':{'group':0,'shape':0},'height':20,'name':'Dimensioned base','color':[.35,.53,.85]}])
view('04_sketch_extruded')
run('Save sketch-solid scene',[{'cmd':'save','path':str((OUT/'MCP_sketch_solid.meltcad').resolve())}])
p.stdin.close();p.wait(timeout=30)
(OUT/'raw_mcp_transcript.json').write_text(json.dumps(records,ensure_ascii=False,indent=2))
(OUT/'readable_mcp_transcript.json').write_text(json.dumps(readable,ensure_ascii=False,indent=2))
(OUT/'stages.json').write_text(json.dumps(stage,indent=2))
(OUT/'verification.json').write_text(json.dumps({'server':hello,'tool_names':[t['name'] for t in tools['tools']],'invalid_edit_rejected':True,'scene_unchanged_after_error':before==after,'binary_sha256':hashlib.sha256(BIN.read_bytes()).hexdigest(),'source_commit':'85d821532928e0efa8f19503da564bf3e5719468','capture_type':'Real headless MCP session, AI-authored client. Not a recording of Claude Desktop.','elapsed_seconds':round(time.monotonic()-started,2)},indent=2))
(OUT/'README.txt').write_text('AUTHENTIC MCP EVIDENCE\nThe client sent actual JSON-RPC requests to the unchanged Melt3D MCP server.\nImages are decoded directly from melt3d_view replies.\nThis proves tool execution, not an autonomous natural-language Claude session.\nSource: 85d821532928e0efa8f19503da564bf3e5719468.\nToken files and private user data are not included.\n')
print('MCP EVIDENCE COMPLETE',flush=True)
