import React from 'react';
import {AbsoluteFill, Audio, Img, OffthreadVideo, Sequence, interpolate, spring, staticFile, useCurrentFrame, useVideoConfig} from 'remotion';
import tl from '../public/timeline.json';
import manifest from '../public/manifest.json';

const FPS = 30;
const BEAT = tl.beat;
const COLORS = ['#ff4f8b', '#19b8e0', '#ffc21a', '#2fd47a', '#8b5cf6', '#ff8a3d'];
const KW = ['ひらめき!', 'サーフェス × ポリゴン', 'まる・しかく→やわらか', '置く', '近づける', 'なめらか', 'とけ具合', '数値で比べる', 'くり抜く', '穴もデザイン', '色とつや', 'プリセット', 'ライト', 'スケッチ→立体', 'SolidCraft', '履歴', '動かす', '書き出し', 'AI連携', 'Melt3D'];
const CLIPS = ['T01', 'T02', 'T03', 'T04', 'T05', 'T06', 'T07', 'T08', 'T09', 'T10', 'T11', 'T12', 'T13', 'T14', 'T15', 'T16', 'T17', 'T18', 'T19', 'T20'];
// crop windows on the 1280x800 recording: [cx, cy, zoom]
const CROPS: Record<string, number[][]> = {
  melt: [[640, 400, 1.0], [620, 420, 1.4], [1110, 330, 1.5], [620, 360, 1.2], [160, 220, 1.5], [800, 440, 1.5]],
  sc: [[640, 400, 1.0], [640, 330, 1.4], [200, 160, 1.5], [900, 420, 1.5], [640, 700, 1.6], [640, 420, 1.2]],
};
const LABEL: Record<string, string> = {mirror: 'ミラーコピー', kind_star: 'ワンタッチで形チェンジ', kind_more: '形がいっぱい', gizmo_move: 'ギズモでぐいっと移動', gizmo_rot: 'ぐるっと回転', gizmo_scale: 'びよーんと拡大', subtract: '引き算でくり抜き', paint: 'ペイント・重ね塗り', shell: 'シェル（中空）', meltface: '面ごとにとかす', ripple: 'さざ波', group: 'グループで整理', library: 'ライブラリ', undo: 'やり直しも自由', quality: '画質きりかえ', viewmenu: '表示メニュー', eye: '表示/非表示', trackpad: 'トラックパッド対応',
  sc_fillet: 'フィレット', sc_chamfer: '面取り', sc_shell: 'シェル', sc_press: 'プレス/プル', sc_move: '移動/コピー', sc_mirror: 'ミラー', sc_pattern: 'パターン', sc_param: 'パラメータ', sc_offset: 'オフセット', sc_hole: '穴', sc_revolve: '回転', sc_inspect: '検査', sc_view: '表示', sc_sheet: '板金', sc_insert: '挿入'};
const FEAT_ORDER = ['mirror', 'sc_fillet', 'gizmo_move', 'sc_pattern', 'kind_star', 'sc_mirror', 'gizmo_rot', 'sc_chamfer', 'subtract', 'sc_shell', 'gizmo_scale', 'sc_move', 'paint', 'sc_hole', 'shell', 'sc_press', 'meltface', 'sc_offset', 'ripple', 'sc_param', 'group', 'sc_revolve', 'library', 'sc_inspect', 'undo', 'sc_sheet', 'kind_more', 'sc_view', 'eye', 'sc_insert', 'trackpad', 'quality', 'viewmenu'];
const getFeat = (name: string) => {
  const clip = name.startsWith('sc_') ? 'FEATS' : 'FEATM';
  const mk: any[] = ((manifest as any)[clip] || {}).marks || [];
  const e = mk.find((m) => m[0] === name);
  return {clip, t: e ? e[1] : 0};
};
const GRID = BEAT * 2;
const stingerStartRel = (i: number) => Math.ceil((tl.sections[i].dur + 0.25) / GRID - 1e-6) * GRID;
const stingerCount = (i: number) => {
  if (i >= tl.sections.length - 1) return 0;
  return Math.max(0, Math.floor((tl.sections[i + 1].start - tl.sections[i].start - stingerStartRel(i)) / GRID + 1e-6));
};
const SC_CLIPS = ['T02', 'T15', 'T16'];

const pulse = (frame: number) => {
  const t = (frame / FPS) % BEAT;
  return Math.exp(-t * 9);
};

const Footage: React.FC<{clip: string; clipTime: number; crop: number[]; rate: number; tilt?: number}> = ({clip, clipTime, crop, rate, tilt = 0}) => {
  const frame = useCurrentFrame();
  const [cx, cy, z0] = crop;
  const z = z0 * (1 + 0.07 * Math.min(1, frame / 40) + 0.025 * pulse(frame));
  // translate so (cx,cy) is centered in 1280x720 view
  const tx = 640 - cx * z, ty = 360 - (cy - 40) * z;
  const tmax = 0, tmin = 0;
  const x = Math.min(0, Math.max(1280 - 1280 * z, tx));
  const y = Math.min(-40 * z, Math.max(720 - 800 * z, ty - 0));
  return (
    <AbsoluteFill style={{overflow: 'hidden', background: '#111'}}>
      <div style={{position: 'absolute', left: 0, top: 0, width: 1280, height: 800, transformOrigin: '0 0', transform: `translate(${x}px, ${ty < -40 * z ? y : ty}px) scale(${z}) rotate(${tilt}deg)`}}>
        <OffthreadVideo src={staticFile(`clips/${clip}.mp4`)} startFrom={Math.max(0, Math.round(clipTime * FPS))} playbackRate={rate} muted style={{width: 1280, height: 800}} />
      </div>
    </AbsoluteFill>
  );
};

const Telop: React.FC<{text: string; color: string; frame: number; size: number; x: number; y: number; rot?: number; stroke?: number}> = ({text, color, frame, size, x, y, rot = -3, stroke = 14}) => {
  const chars = Array.from(text);
  return (
    <div style={{position: 'absolute', left: x, top: y, transform: `rotate(${rot}deg)`, display: 'flex', fontFamily: '"Noto Sans CJK JP", sans-serif', fontWeight: 900, fontSize: size, lineHeight: 1.05, whiteSpace: 'pre'}}>
      {chars.map((c, i) => {
        const s = spring({frame: frame - i * 2, fps: FPS, config: {damping: 9, stiffness: 190, mass: 0.6}});
        const sc = interpolate(s, [0, 1], [0, 1]);
        return (
          <span key={i} style={{display: 'inline-block', transform: `translateY(${(1 - s) * -60}px) scale(${sc}) rotate(${(1 - s) * 18}deg)`, color: '#fff', WebkitTextStroke: `${stroke}px ${color}`, paintOrder: 'stroke fill', textShadow: `0 ${size * 0.06}px 0 rgba(0,0,0,.28)`}}>{c}</span>
        );
      })}
    </div>
  );
};
const SubPill: React.FC<{text: string; color: string; frame: number}> = ({text, color, frame}) => {
  const s = spring({frame: frame - 14, fps: FPS, config: {damping: 12, stiffness: 160}});
  return (
    <div style={{position: 'absolute', left: 44, top: 142, transform: `translateX(${(1 - s) * -400}px) rotate(-1.5deg)`, background: '#fff', color: '#222', fontFamily: '"Noto Sans CJK JP"', fontWeight: 900, fontSize: 27, padding: '8px 22px', borderRadius: 40, border: `5px solid ${color}`, boxShadow: '0 5px 0 rgba(0,0,0,.25)'}}>{text}</div>
  );
};

const POSES = ['idle_c', 'idle_o', 'blink', 'wave_c', 'wave_o', 'cheer_c', 'cheer_o', 'tiltL_c', 'tiltL_o', 'tiltR_c', 'tiltR_o', 'hop_o'];
const Mascot: React.FC<{frame: number; secFrame: number; secIdx: number}> = ({frame, secFrame, secIdx}) => {
  const amp = tl.amp[Math.min(tl.amp.length - 1, frame)] || 0;
  const open = false;
  const beatN = Math.floor((frame / FPS) / BEAT);
  const side = beatN % 2 === 0 ? 'tiltL' : 'tiltR';
  let pose = open ? `${side}_o` : `${side}_c`;
  if (secFrame < 18) pose = open ? 'wave_o' : 'wave_c';
  if (secIdx === 19) pose = open ? 'cheer_o' : 'cheer_c';
  else if (!open && frame % 100 < 4) pose = 'blink';
  
  const bob = -amp * 22 - pulse(frame) * 10;
  const sq = 1 + pulse(frame) * 0.04 - amp * 0.03;
  return (
    <div style={{position: 'absolute', right: 24, bottom: -6, width: 190, height: 312, transform: `translateY(${bob}px) scale(${1 / sq}, ${sq})`, transformOrigin: '50% 100%', filter: 'drop-shadow(0 0 0 #fff) drop-shadow(3px 0 0 #fff) drop-shadow(-3px 0 0 #fff) drop-shadow(0 3px 0 #fff) drop-shadow(0 -3px 0 #fff) drop-shadow(0 8px 10px rgba(0,0,0,.35))'}}>
      <Img src={staticFile(`mascot/${pose}.png`)} style={{width: '100%', height: '100%', objectFit: 'contain'}} />
    </div>
  );
};

const Flash: React.FC<{frame: number; color: string}> = ({frame, color}) => {
  const o = interpolate(frame, [0, 6], [0.9, 0], {extrapolateRight: 'clamp'});
  return <AbsoluteFill style={{background: color, opacity: o, mixBlendMode: 'screen'}} />;
};
const Wipe: React.FC<{frame: number; color: string}> = ({frame, color}) => {
  if (frame > 14) return null;
  const p = interpolate(frame, [0, 14], [0, 1]);
  const x = interpolate(p, [0, 1], [-1400, 1500]);
  return <div style={{position: 'absolute', top: -50, left: x, width: 1100, height: 900, background: color, transform: 'skewX(-18deg)'}} />;
};
const Speed: React.FC<{frame: number}> = ({frame}) => {
  if (frame > 8) return null;
  const o = interpolate(frame, [0, 8], [0.55, 0]);
  return <AbsoluteFill style={{opacity: o, background: 'repeating-conic-gradient(from 0deg at 50% 50%, #fff 0deg 1.2deg, transparent 1.2deg 7deg)', WebkitMaskImage: 'radial-gradient(circle at 50% 50%, transparent 30%, #000 85%)'}} />;
};

const Code: React.FC<{frame: number}> = ({frame}) => {
  const lines = ['$ claude mcp add melt3d \\', '    -- meltcad --mcp --control 7777', '', '> 球を2つ置いて、とかして', '✓ shape.add  Sphere ×2', '✓ melt.set   amount 1.4'];
  const n = Math.floor(frame * 1.6);
  let left = n; const out: string[] = [];
  for (const l of lines) { out.push(l.slice(0, Math.max(0, left))); left -= l.length; }
  return (
    <div style={{position: 'absolute', left: 40, bottom: 40, width: 560, background: 'rgba(16,20,32,.93)', borderRadius: 18, border: '4px solid #19b8e0', padding: '18px 22px', fontFamily: 'DejaVu Sans Mono, monospace', fontSize: 21, color: '#c9f', lineHeight: 1.45, boxShadow: '0 8px 0 rgba(0,0,0,.3)', whiteSpace: 'pre'}}>
      {out.map((l, i) => <div key={i} style={{color: l.startsWith('✓') ? '#4ade80' : l.startsWith('>') ? '#ffd166' : '#a5b4fc'}}>{l || ' '}</div>)}
    </div>
  );
};

export const Main: React.FC = () => {
  const frame = useCurrentFrame();
  const secs = tl.sections;
  const t = frame / FPS;
  let idx = 0;
  for (let i = 0; i < secs.length; i++) if (t >= secs[i].start - 0.0) idx = i;
  const sec = secs[idx];
  const next = secs[idx + 1];
  const secEnd = next ? next.start : tl.total;
  const secFrame = Math.max(0, frame - Math.round(sec.start * FPS));
  const relRaw = t - sec.start;
  const rel = Math.max(0, relRaw);
  const voiceEnd = sec.dur + 0.25;
  const clip = CLIPS[idx];
  const info = (manifest as any)[clip] || {dur: 10};
  const kind = SC_CLIPS.includes(clip) ? 'sc' : 'melt';
  const crops = CROPS[kind];
  const color = COLORS[idx % COLORS.length];
  const shotLen = BEAT * 4;
  const lastSec = idx === secs.length - 1;
  const sStart = lastSec ? 9999 : stingerStartRel(idx);
  const nSt = stingerCount(idx);
  const inStinger = !lastSec && nSt > 0 && rel >= sStart;
  let before = 0; for (let j = 0; j < idx; j++) before += stingerCount(j);
  const kSt = inStinger ? Math.min(nSt - 1, Math.floor((rel - sStart) / GRID)) : 0;
  const featName = FEAT_ORDER[(before + kSt) % FEAT_ORDER.length];
  const feat = getFeat(featName);
  let shotIdx: number, shotStart: number;
  if (inStinger) { shotIdx = 100 + kSt; shotStart = sStart + kSt * GRID; }
  else { const lim = lastSec ? 9999 : sStart; shotIdx = Math.floor(Math.min(rel, lim - 0.01) / shotLen); shotStart = shotIdx * shotLen; }
  const shotFrame = relRaw < 0 ? frame : Math.round((rel - shotStart) * FPS);
  const rate = Math.max(0.8, Math.min(2.2, (info.dur * 0.92) / Math.max(4, (sStart < 9000 ? sStart : voiceEnd + 3))));
  const clipTime = Math.min(info.dur - 1.2, 0.4 + rel * rate);
  const crop = crops[(shotIdx + idx) % crops.length];
  const tilt = (shotIdx % 2 === 0 ? 1 : -1) * (inStinger ? 1.0 : 0.0);
  const intro = t < sec.start - 0.01 && idx === 0;
  return (
    <AbsoluteFill style={{background: '#fff'}}>
      <Audio src={staticFile('mix.wav')} />
      <Sequence key={`${idx}-${shotIdx}`} from={relRaw < 0 ? 0 : Math.round((sec.start + shotStart) * FPS)} layout="none">
        {inStinger
          ? <Footage clip={feat.clip} clipTime={Math.max(0, feat.t - 0.15)} crop={[640, 400, 1.0 + 0.05 * (kSt % 3)]} rate={1} tilt={tilt} />
          : <Footage clip={clip} clipTime={Math.min(info.dur - 1.5, 0.4 + shotStart * rate)} crop={crop} rate={rate} tilt={tilt} />}
      </Sequence>
      <Speed frame={shotFrame} />
      <Flash frame={shotFrame} color={shotIdx % 2 ? '#fff' : color} />
      <Wipe frame={secFrame} color={color} />
      {/* heading telop (voice part) */}
      {relRaw >= 0 && rel < voiceEnd + 0.1 && (
        <>
          <Telop text={sec.head} color={color} frame={secFrame - 3} size={sec.head.length > 12 ? 58 : 70} x={40} y={38} rot={-2.5} />
          <SubPill text={sec.sub} color={color} frame={secFrame} />
        </>
      )}
      {/* stinger keyword */}
      {inStinger && (
        <>
          <Telop key={kSt} text={LABEL[featName] || featName} color={COLORS[(before + kSt) % COLORS.length]} frame={shotFrame} size={78} x={44} y={470} rot={-3} stroke={16} />
          <div style={{position: 'absolute', left: 44, top: 36, fontFamily: '"Noto Sans CJK JP"', fontWeight: 900, fontSize: 30, color: '#fff', background: COLORS[(before + kSt) % COLORS.length], padding: '4px 18px', borderRadius: 30, border: '4px solid #fff', boxShadow: '0 4px 0 rgba(0,0,0,.3)', transform: 'rotate(-2deg)'}}>機能 {String(before + kSt + 1).padStart(2, '0')}</div>
        </>
      )}
      {idx === 18 && rel < voiceEnd && rel > 1.0 && <Code frame={Math.round((rel - 1.0) * FPS)} />}
      {lastSec && rel > voiceEnd && (
        <AbsoluteFill style={{alignItems: 'center', justifyContent: 'center', background: 'rgba(20,20,40,.55)'}}>
          <Telop text="Melt3D" color="#ff4f8b" frame={Math.round((rel - voiceEnd) * FPS)} size={190} x={330} y={190} rot={-3} stroke={26} />
          <div style={{position: 'absolute', bottom: 120, left: 0, width: 1060, textAlign: 'center', fontFamily: '"Noto Sans CJK JP"', fontWeight: 900, fontSize: 34, color: '#fff', textShadow: '0 3px 0 #000'}}>github.com/07egta/melt3d</div>
        </AbsoluteFill>
      )}
      <Mascot frame={frame} secFrame={secFrame} secIdx={idx} />
      <div style={{position: 'absolute', left: 16, bottom: 10, fontFamily: '"Noto Sans CJK JP"', fontWeight: 700, fontSize: 16, color: '#fff', textShadow: '0 1px 3px #000, 0 0 6px #000'}}>VOICEVOX:ずんだもん　/　Melt3D 江口太一朗</div>
      {frame < 26 && <Telop text="Melt3D" color="#ff4f8b" frame={frame} size={220} x={300} y={220} rot={-4} stroke={28} />}
    </AbsoluteFill>
  );
};
