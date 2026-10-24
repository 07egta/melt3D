<h1 align="center">Melt 3D</h1>

<p align="center">
  <strong>やわらかい形を かんたんに作れる 3D モデリングアプリ</strong><br />
  An easy 3D modeller where shapes melt into each other, with CAD-style dimensions and a full
  precise-solid mode. Written in Rust. Free and open source (MIT OR Apache-2.0).
</p>

## ダウンロード / Download

[Releases](https://github.com/07egta/melt3D/releases/latest) から、自分のパソコン用のファイルをダウンロードします。
Download the file for your computer from [Releases](https://github.com/07egta/melt3D/releases/latest).
Windows の「PCが保護されました」は、署名していない個人製作のアプリのときに出ます。信用できる場合だけ「詳細情報」→「実行」。

## できること / What it does

- 形どうしが とけて くっつく（Melt）、小さい形で くり抜く
- 絵をかいて押し出す（スケッチ → 押し出し）、曲線とパイプ、アニメーション
- 色・つや（ざいしつ）、STL / 3MF / OBJ / PLY / GLB への書き出し（3D プリント向けの点検つき）
- **SolidCraft モード**: 拘束つきスケッチとパラメトリックな履歴を持つ精密な立体作りを、そのまま使えます。作った立体は Melt に取り込めます

操作の説明は [apps/meltcad/README.md](apps/meltcad/README.md)。

## 土台にしているもの / Built on SolidCraft

Melt 3D は、オープンソースの CAD **SolidCraft**（<https://github.com/tangency-labs/solidcraft>、
MIT OR Apache-2.0）のソースコードを取り込み、その上に作った別のプログラムです。
`crates/`、`vendor/`、`xtask/` などは SolidCraft のコードで、Melt 3D 側の追加は主に `apps/meltcad/` にあります。
SolidCraft の作者や Melt 3D の利用者以外の会社・団体とは**無関係**で、公式のものでも、承認を受けたものでもありません。
SolidCraft の README は [docs/SOLIDCRAFT-UPSTREAM-README.md](docs/SOLIDCRAFT-UPSTREAM-README.md) に残してあります。

## ライセンス / Licence

[MIT](LICENSE-MIT) または [Apache-2.0](LICENSE-APACHE) のどちらかを選べます。
著作権表示は [NOTICE](NOTICE) と [LICENSE-MIT](LICENSE-MIT)、使っている画像・アイコンは [ATTRIBUTION.md](ATTRIBUTION.md)、
使っているライブラリのライセンスは [THIRD-PARTY-LICENSES.txt](THIRD-PARTY-LICENSES.txt) に書いてあります。
**無保証**です（作品の消失や破損について、作者は責任を負いません）。大切なデータは、こまめに別の場所にも保存してください。

## 他社製品との関係 / Independence and trademarks

- Melt 3D は、商用 CAD や 3D ソフトのコード・アイコン・画像・文章・テンプレートを使っていません。アイコンはコードで描くか、独自に作ったものです。
- 他の製品名（FreeCAD、Rhino、STEP など）は、読み込みや書き出しができるファイル形式を示すためだけに書いてあり、
  それぞれの持ち主の商標です。その会社とは関係がありません。
- This project is independent of, and not affiliated with or endorsed by, any CAD vendor or the SolidCraft authors.
  All product names are trademarks of their owners and are used only to name file formats.
- 問題に気づいたら（権利に関するものも）、[Issues](https://github.com/07egta/melt3D/issues) に書いてください。すぐ直します。

*This is not legal advice.*
