# melt3D 紹介動画（共同編集用）

高校生 × AI で作った 3D モデラー **melt3D** の紹介動画（約 70 秒）の台本・絵コンテ・制作ツール一式です。
声は VOICEVOX:ずんだもん。映像は **実際の melt3D の画面録画**（最新版）＋カラフルなアニメーションカードで構成します。

## ここを編集すれば動画が変わる
| ファイル | 役割 |
|---|---|
| `narration.json` | 全カットの定義（id / 種別 / 画面に出すタイトル / ナレーション / 録画の切り出し位置 `src` `ss`） |
| `script.md` | 脚本（人が読む用。`narration.json` から作った表） |
| `storyboard/storyboard.md` | 絵コンテ（各カットの静止画つき） |
| `voicevox_lines.txt` | VOICEVOX に貼るセリフ（1行1カット） |
| `contests.md` | 応募先コンテストの締切・条件メモ |
| `tools/` | 録画（`take_*.py`）と合成（`compose.py`）のスクリプト |

## 作り方
1. 最新の melt3D をビルドし、`tools/serve.sh` で仮想ディスプレイ上に起動（`--control 7788`）。
2. `python3 tools/driver.py <名前> tools/take_<名前>.py` で操作を自動実行しつつ画面を録画 → `clips/<名前>.mp4`。
3. VOICEVOX（ずんだもん）で `voicevox_lines.txt` を読み上げ、`audio/<id>.wav` として保存。
4. `python3 tools/compose.py` → `out/melt3d_intro.mp4` と `.srt`。声があればその長さにカットが自動で合わせられます。

※ スクリプトは作業環境のパス（`/home/claude/...`）を前提にしています。自分の環境に合わせて書き換えてください。
※ 主人公キャラのシーンファイル（`.meltcad`）はリポジトリに含めていません（ずんだもんの二次創作ガイドラインに沿って各自で用意）。

## ルール
- 動画内のクレジット: `VOICEVOX:ずんだもん`
- 画面は最新の `main` のビルドで撮り直すこと（古い版の画面を混ぜない）。
- 画面に出す文字は最小限（字幕は焼き込まず `.srt` で別出し）。
