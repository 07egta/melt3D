# melt3D 紹介動画 v2（アニメ風・約2分50秒）

参考動画（ChatGPT製20セクション）の構成を土台に、**パワポ風をやめて**、ビートに合わせた短いカット・太字テロップ・跳ねるずんだもんで作り直した版。

- 尺: 170秒 / 128 BPM。声は1.3倍速。セクション間に「機能バースト」（ミラー・ギズモ・パターン・フィレット等を1秒ずつ）。
- 操作画面はすべて実機（`tools/record.py` + `batch*.py`、仮想画面Xvfb上でmeltcadを自動操作して録画）。
- 合成は Remotion（`remotion/`）。`tools/mix.py` が声のタイミング＋BGMを作る。
- 声: VOICEVOX:ずんだもん。キャラは自作（.meltcad は含めない）。

## 作り方
1. `tools/record.py batchN` で操作画面を録画 → `clips/*.mp4`（+`*.marks.json`）
2. `tools/mix.py` で `public/timeline.json` と `mix.wav` を作成
3. `tools/mkmanifest.sh` で `public/manifest.json` 作成
4. `cd remotion && npx remotion render src/index.ts Melt3D out.mp4`
