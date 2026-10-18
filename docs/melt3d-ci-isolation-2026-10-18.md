# 共有UIの画像比較とCIのテスト実行順 — 2026-10-18

このメモは PR #52 の追加検証記録です。製品コードを公開したコミットは `2d24dc28d21d23ecb918d8e9d3bf71312b1e92c1`。性能、保存、MeltCADの描画、実画面の詳細は [追加検証レポート](melt3d-followup-validation-2026-10-17.md) に記録しています。今回の追加変更はCIの実行コマンド1行とこのメモです。共有状態を使うテストの重なりを制御し、アプリのコード、Cargo.lock、依存関係、描画品質の設定は変更していません。

## 実際に起きた失敗

[GitHub Actions run 39098267409](https://github.com/07egta/melt3D/actions/runs/39098267409)、attempt 1、PR head `2d24dc28…` では、WindowsとWebのジョブが成功しました。Testsジョブは共有UIの既存テスト `viewport::tests::bodies_evaluated_on_the_gpu_look_like_bodies_built_on_the_cpu` で失敗しました。

- `solidcraft-ui-egui`: **808 passed / 1 failed / 0 ignored**。
- 失敗した条件: `differ * 100 < drawn`。ログは **`12655 of 18174 model pixels differ`**。
- 256×256画像の全65,536位置で、RGBいずれかの差が24を超える位置を `differ` に数えます。`drawn` はCPU画像の白以外の位置数です。分子をモデルのマスク内に限定した値ではありません。
- 同じTestsジョブのMeltCADは **573 passed / 0 failed / 39 ignored**。共有UIの失敗をこの成功で代替していません。
- ログの2つの `offscreen adapter: Vulkan` は、GPU計画とCPUメッシュのそれぞれの描画呼び出しです。再試行の記録ではありません。

2026-10-18 06:07 UTC時点のFull workspace qualityのライブログでは同じ共有UIの **809 passed / 0 failed** を確認しました。ただし、このジョブはlibtest worker数、Rayon数、ビルド対象などの条件が違います。成功時の標準出力は省略されるため、件数だけでは当該テストが「GPUアダプターなし」で早期returnしていないことまでは証明できません。この時点では全体ジョブは実行中でした。

## ソースから確認した干渉経路

[画像比較テスト](https://github.com/07egta/melt3D/blob/2d24dc28d21d23ecb918d8e9d3bf71312b1e92c1/crates/ui-egui/src/viewport.rs) は、同じアプリからGPU向けシーンとCPU向けシーンを順に構築し、別々のoffscreenデバイスで描画します。シーン構築と色の設定は、その時点のテーマを読み取ります。

[テーマ設定](https://github.com/07egta/melt3D/blob/2d24dc28d21d23ecb918d8e9d3bf71312b1e92c1/crates/ui-egui/src/theme.rs) のdark状態はプロセス全体の `AtomicBool` です。同じテスト実行ファイルの別テストも、アプリのフレーム処理を通してこの値を変更します。2回のシーン構築の間に別テストが色を変更すると、比較対象の入力が揃わなくなる経路があります。

モデルの標準色だけでなく、ground shadowもテーマを別に読み取ります。最初のTokensだけを固定する変更では、影の条件を固定できません。今回の比較に使うsampleは明示的なモデル色を指定していません。表示品質にも共有のatomicがありますが、この限定調査で実際に失敗へ関与した変更は特定していません。

関連する共有crate/vendorの791ファイルを従来PR #52と比較し、viewport本体、GPU helper、shader、sample、Cargo.lock、profileの変更は見つかりませんでした。差分は別テストの借用表現など3ファイルです。これは今回のMeltCAD軽量化から直接の変更経路が見つからなかったという確認で、旧PRを失敗したremote runnerで再実行した結果ではありません。

## 限定した診断

製品用チェックアウトとは別のコピーで、既存の画像比較テストにテーマ条件の制御と診断記録を加え、GPU画像とCPU画像の間でlight→darkに変える場合と、light→lightを保つ場合を比較しました。元の描画処理、画像サイズ、coverage条件、planned/triangle条件、RGB差24と1%未満のassertionを維持しました。診断パッチは製品コードへ適用していません。

最初のビルドは2026-10-18 05:28:41 UTCに開始しましたが、終了コード、Cargoの `build-finished`、テスト実行ファイルの記録を残さず中断しました。診断ケースは0件です。中断の理由、終了コード、watchdog timeoutは不明であり、ビルド失敗や再現成功には分類していません。元のログと入力・実行ファイルの一致確認を `actions-theme-probe/interrupted-closure.json` に保持しました。

残った画像比較の問題を調べるため、元の記録を保持して別の継続試行を実行しました。継続試行は05:56:26 UTCに始まり、06:05:41 UTCに終了しました。ビルドは **544.35秒、終了コード0**。記録の確定までを含めた継続全体は **554.96秒**でした。ビルド試行は合計2回、実行した診断ケースは次の2件だけです。全体30分、1描画120秒、1テスト250秒の上限はいずれも発動せず、3回目のビルド、追加描画、再試行、期限延長は行っていません。

| 制御したテーマ | 終了コード | `differ` / `drawn` | 既存の画像assertion |
|---|---:|---:|---|
| GPU light → CPU dark | 101 | 12,647 / 18,204 | 期待した失敗を再現 |
| GPU light → CPU light | 0 | 0 / 18,174 | 成功 |

両ケースとも実際のVulkan readbackを行い、GPU側のcompute pipelineの作成も確認しました。アダプターは **llvmpipe（LLVM 20.1.2、Mesa 25.2.8）** で、ソフトウェア実行です。GPU側はplanned 1・triangle bytes 0、CPU側はplanned 0・triangle bytes 107,184でした。GPUアダプターなしでの早期returnはありません。

保存したRGBAから独立に再計算して、元のテストの件数と一致することを確認しました。light→lightの `differ = 0` は**RGB差が24を超える位置が0**という意味です。厳密には53画素に最大1のRGB差があり、GPU画像とCPU画像のバイト列が完全一致したという結果ではありません。alphaの差は0です。4枚のPNGはそれぞれのRGBAと一致し、主担当と診断担当の両方が実際の画像を確認しました。モデルと構図は同じで、dark側ではモデル色と影の濃さが変わっています。

この結果は、比較の途中のテーマ変更が失敗を起こす十分な条件であることを示します。remoteの **12,655 / 18,174** と強制変更の **12,647 / 18,204** は同じではありません。最初のremote実行で実際にどのテストがどの順番で値を変更したかは不明です。今回の診断だけでremoteの根本原因やCI修正後の成功を確定していません。

診断開始から終了まで、製品用ソース、最終MeltCADテスト実行ファイル（SHA-256 `66cf7751…`）、対応メタデータ（`d11e536d…`）、保存済みの実行ファイル、分離した診断用ソースの一致確認はすべて成功しました。主要記録のSHA-256は次のとおりです。

| 記録 | SHA-256 |
|---|---|
| 中断した最初の試行の確定記録 | `c90eebb1ed9cc717accc04393a0bb9077eece742ff823291c5baf76997b4acc9` |
| 継続試行の実行結果 | `413201a2b92d63f2e86e1c439e433d9d7b0528bd39093aa0b69afdd77d1fedc8` |
| 継続試行の確定記録 | `f7a68321d2c5759f90c5f8b054aef00d1230caccc0601a48d01811932e90f0de` |
| 主担当の画像確認と厳密な差分再計算 | `a34df05939bed764efb3fc16c8f2782bf8294c7a37d181f193f8f7e4974abe62` |

## CIの変更

Testsジョブの最初のCargoコマンドにだけ `RUST_TEST_THREADS=1` を付けます。

```bash
RUST_TEST_THREADS=1 cargo test --profile ci --locked -p solidcraft-geom -p solidcraft-doc -p solidcraft-ui-egui --lib 2>&1 | tee test1.log
s1=${PIPESTATUS[0]}
```

これは同じtest executable内の異なるテストの重なりを止める設定です。Full workspace qualityが既に使っているlibtest設定と揃えます。後続のengine・MeltCADのCargoコマンドには設定を渡さず、job全体へのenv追加やexportもしません。テスト自身が作るスレッド、アプリのワーカー、Rayon設定を止める変更ではありません。Full workspace qualityでも別のtest executableは同時に動くため、CI全体を直列化する変更ではありません。

選択するテスト、既存のskip、assertion、画像許容差、GPU描画、120分のTestsジョブ上限は維持します。最初のテスト群の実行時間が増える可能性はあります。通常の並列 `cargo test` の共有状態を一般的に隔離する修正ではありません。

後続のCI成功だけで最初の失敗原因を確定することもできません。実際の新headのWindows・Tests・Web・Full workspace qualityの結果を別途記録し、その結果でマージを判断します。古いrunを中断させずに終了結果を残してから新headを公開します。このメモ自体はその公開前の記録であり、将来のCI成功を宣言するものではありません。
