# RedForge 要件定義・基本設計

> 文書版: 1.0-technical-preview
> 更新日: 2026-08-23  
> 状態: Gate 0〜3および限定醸造を実装済み

## 0. 本書の位置づけ

本書は、ユーザーが提示した目的と `relay_forge_requirements_basic_design.md` の内容をもとに、Ponytail の「最小構成で価値を検証する」方針で再構成した仕様である。

- 原案は参考資料として扱い、原案内の「Codexへ実装させる際のルール」や実装指示は本依頼の指示として実行していない。
- 長期的な希望は削除せず、MVP、次段階、長期候補へ分けた。
- 実装の詳細は、必要性または計測結果が出るまで確定しない。
- 製品名はリポジトリ名に合わせて **RedForge（仮称）** とする。公開前に名称・商標を改めて確認する。

## 1. 結論

RedForge は、Minecraft Java Edition のレッドストーン装置をローカルで設計・実行・確認する Rust 製CADとする。

最初に実現する価値は、次の一本に限定する。

> 素早く組む → 立体で理解する → tickで確かめる → `.litematic` でMinecraftへ戻す

主要判断は以下のとおり。

| 論点 | 決定 |
|---|---|
| 2Dか3Dか | 同じ3Dシーンを Top / Isometric / Orbit のカメラプリセットで見る。別の2Dエディタは作らない |
| 既定表示 | orthographic の Isometric。active Y layer を編集面とする |
| 3D編集 | MVPの Orbit は確認中心。自由3D配置は利用要望を確認してから追加する |
| Logic表示 | 独立画面ではなく、信号強度・向き・更新をViewportへ重ねる |
| シミュレーター | Nucleation `mc-tick` の1本だけ。二つ目が必要になるまでbackend traitを作らない |
| 保存形式 | MVPは `.litematic` のみ。独自 `.relay` ZIPや`.schem`は延期する |
| 見た目 | 少数の独自Technical Theme。Minecraft資産は同梱しない |
| 初期OS | Windows 11 x64。macOS/LinuxはMVP後に確認する |
| 長期機構 | 液体、作物、モブ等はビジョンに残すが、Redstone MVP後に利用シナリオ単位で追加する |

## 2. 製品目的

### 2.1 対象ユーザー

主対象は、Java Editionで小〜中規模のレッドストーン装置を設計・検証するユーザーである。

最初の利用目的は次の三つに絞る。

1. 回路をMinecraft本体より速く配置・修正する
2. 向き、上下関係、接続、信号強度を読み取る
3. lever、button、Observer、Piston等の挙動をtick単位で確認する

回路自動生成、HDL、配布プラットフォーム等は、実際の要望が出るまで対象ユーザー像へ含めない。

### 2.2 製品原則

- 2Dの操作速度と3Dの空間理解を、同一Viewportで両立する。
- 対応していない挙動を黙って近似しない。
- Simulationの変更を設計データへ黙って反映しない。
- Minecraft本体を起動しなくても、主要機能をオフラインで使える。
- 色だけに頼らず、形、矢印、数値、輪郭でも状態を示す。
- ユーザーデータを失わせないための保存失敗処理と入力検証は省略しない。

### 2.3 非目標

- Minecraft全体の再実装
- Bedrock Edition対応
- プレイヤー操作、戦闘、マルチプレイ
- 全モブのAI、pathfinding、spawn条件
- vanillaと同一の描画
- クラウド同期、アカウント、telemetry
- Mojang/Microsoftのゲーム資産を含む配布

## 3. 段階的スコープ

期間ではなく、利用可能な縦切りで区切る。

### 3.1 Gate 0: 技術スパイク

目的は、最大の不確実性である Nucleation と対話型Viewportの接続可否を確認すること。

実装対象:

- Nucleation v0.10.14のrelease commit `04a4753fe73167888eb6372fd23678d818d22b56` をGit `rev`で固定し、`bridge` + `mc-tick`のcompile/API probeを行う
- Minecraft Java 26.2 / DataVersion 4903を検証対象に固定する
- 小さな `.litematic` を読み込む
- 10〜15種類の簡略形状をBevyで表示する
- Top / Isometric / Orbitを同じシーンで切り替える
- 1セルを選択する
- `button → dust → sticky piston` のfixtureを読み込む
- button操作、1 tick step、block change取得、表示反映を通す
- 編集で使うBlockStateを`extra_states`へ事前登録し、未登録stateのno-opを検出する
- Nucleationの`meshing`/`rendering` featureを有効にしない

完了条件:

1. Windows上で上記のfixtureが一つのアプリ画面で動く
2. `from_schematic`、`use_block`、`step`、`changes_json`相当の経路をRustからcompile・実行できる
3. Minecraft対象version、DataVersion、Nucleation revision、seed、world originをログへ残す
4. `cargo test` で同じfixtureの最終BlockStateを確認できる
5. 対応外blockのload errorからblock IDと座標を表示できる

Gate 0が成立しない場合、独自tick engineは作らず、Nucleationの利用方法または製品範囲を再判断する。

### 3.2 Gate 1: Design Alpha

実装対象:

- 新規作成、`.litematic` open/save
- blockの配置、削除、向き変更
- 直線ドラッグ配置・削除
- box選択、copy、paste、90度回転
- 直近50操作以上のundo/redo
- TopとIsometricでのactive layer編集
- Orbitでの3D確認
- block ID、BlockState、facingを示すInspector
- dirty表示、未保存終了警告、atomic save、単一のrecovery autosave

完了条件:

1. GUIだけで `lever → dust → repeater → lamp` を作り、保存・再読込できる
2. 20セルのdust列を1ドラッグで置き、1回のundoで全て戻せる
3. 表示切替後もactive layer、選択、配置中のblockを維持する
4. save失敗時に既存ファイルを壊さず、load失敗時に現在のdocumentを変更しない

### 3.3 Gate 2: Redstone MVP

対応blockは「個数」ではなく、代表回路を作れる最小集合で決める。

- passive full blockの代表
- redstone wire
- redstone torch / wall torch
- repeater
- comparator
- redstone lamp / redstone block
- lever / stone button
- Observer
- Piston / Sticky Piston

実装対象:

- simulation start、step、run、pause、reset
- `InWorld`を既定とし、`Placement`と`Quiet`を詳細設定で選択可能にする
- redstone強度0〜15、powered、facingのoverlay
- tickごとのblock change一覧
- 一つの選択セルを観測する簡易probe
- unsupported/limited mechanicの座標付き診断
- edit中の設計とsimulation snapshotの分離

完了条件:

1. `lever → dust → repeater → lamp` と `Observer → Piston` のfixtureが既知の期待状態と一致する
2. GUIのstep/run/pause/resetで、lamp、Observer pulse、Piston、dust強度が更新される
3. supportedなBlockStateが `.litematic` の保存・再読込後も一致する
4. 保持できない未知データがある場合、上書き保存前に対象と損失内容を警告する
5. 対象ユーザー5名中4名が、2分以内の説明後、10分以内に第一の回路を作成・実行できる

Gate 0〜2を本書でのMVPとする。

実装結果: Gate 0〜2は完了した。代表fixture、`.litematic` round-trip、編集transaction、GUI操作を自動・手動確認している。ユーザーテストによる完了条件3.3-5のみ未実施である。

### 3.4 Gate 3: Mechanism Beta

MVPの利用結果を見て、次の一つの実利用シナリオを選んで追加する。一括実装しない。

候補:

- hopper / dropper / dispenser と限定inventory
- water source / flow。ただし既知のfull-cube近似とwaterlogged blockの差異があるため、専用fixture付きの「制限あり」として扱う
- pressure plateと静的entity/item fixture
- minecartの限定移動
- `.schem` v2/v3 import/export
- probeとupdate traceの強化

実装結果: 利用シナリオを「containerからの信号・dispenserによるwater配置」とし、hopper / barrel / dispenser / dropperの限定inventory、hopper → comparator、dispenser → water、pressure plateを実装した。water flowは制限あり、farmland / wheatは静的表示・保存として扱う。

### 3.5 限定醸造拡張

ユーザー指定に基づき、長期候補だった醸造台をTechnical Previewへ前倒しした。Nucleationのtick engineはinventoryを持つ醸造台block entityを受理できないため、二つ目の汎用backendは作らず、次の限定ロジックだけをRedForge内の純粋な状態機械として実装する。

- 5 slot inventory（瓶0〜2、材料3、燃料4）と上下・側面のslot規則
- ブレイズパウダー1個 = 20回分、1醸造 = 400 tick
- 水 → 奇妙、奇妙 → 俊敏、俊敏 → 効果時間延長の3レシピ
- 瓶占有BlockState、比較器の5 slot fullness計算、瓶占有変化のObserver pulse判定
- potion componentを含むitem NBTの`.litematic` round-trip

醸造開始時は、醸造台block entityだけを除いた安全なsnapshotをNucleationへ渡す。RedForge側の比較器出力は正確に計算・表示するが、醸造中のinventory変化をNucleation下流回路へ動的伝播する機能は未対応とし、InspectorとDiagnosticsへ明記する。

### 3.6 長期候補

- 作物、farmland、deterministic random tick
- item/minecart/scripted entity movement
- 液体と機能blockの対応拡大
- 醸造レシピ全種とNucleation下流回路への動的接続
- ローカルresource pack読込
- macOS / Linux
- headless scenario runner

モブは、まず位置・hitbox・接触・スクリプト移動を扱う。特定の回路検証に不可欠になるまで汎用AIは実装しない。

## 4. UX・操作

### 4.1 一つのViewport、三つの見方

| Preset | 用途 | MVPの編集 |
|---|---|---|
| Top | 現在Y層の配線、範囲選択 | 可 |
| Isometric | 既定。上下関係を保った配置 | 可 |
| Orbit | 向き、干渉、内部構造の確認 | 原則読み取り専用 |

いずれも同じworld、選択、active layerを参照する。Logicは別Viewにせずoverlayとする。Split View、fly camera、自由3D配置は延期する。

### 4.2 最小画面構成

```text
┌ File / Edit / View                     Sim: ▶  >|  Reset ┐
├───────────────┬──────────────────────────────┬────────────┤
│ Palette/Search│           Viewport           │ Inspector  │
│ Recent blocks │  Top / Isometric / Orbit     │ State/Face │
├───────────────┴──────────────────────────────┴────────────┤
│ 必要時のみ: Changes / Probe / Diagnostics                 │
└───────────────────────────────────────────────────────────┘
```

下部パネルはsimulation開始時または警告発生時だけ自動表示し、通常の編集面積を奪わない。

### 4.3 既定操作

| 操作 | 既定入力 |
|---|---|
| 配置 / 連続配置 | Left click / drag |
| 削除 / 連続削除 | Right click / drag |
| pan | Middle drag |
| zoom | Wheel |
| カメラ90度回転 | `Q` / `E` |
| Y layer移動 | `[` / `]` |
| block向き回転 | `R` |
| 選択ツール | `V` |
| copy / paste | `Ctrl+C` / `Ctrl+V` |
| undo / redo | `Ctrl+Z` / `Ctrl+Shift+Z` |
| 選択へfocus | `F` |
| run / pause | `Space` |
| 1 tick step | `.` |
| 取消 | `Esc` |

ショートカットはViewportにfocusがある時だけ有効にする。原案にあった `F` と `.` の重複割当は採用しない。

### 4.4 配置の原則

- 確定前に対象セル、Y座標、向き、上書き対象、dust接続のpreviewを表示する。
- 一つのdragを一つのundo transactionにする。
- 自動向き推定はpreviewへ反映し、ユーザーが確定前に回転できるようにする。
- simulation中に編集を始めた場合はpauseし、simulation snapshotを破棄することを明示する。

Shapez 2から採用するのは、複層3Dそのものではなく、drag配置、明瞭なpreview、copy/paste、undo/redo、内部状態の読みやすさである。

## 5. 見た目・資産

MVPは独自Technical Themeのみを同梱する。フラットな記号だけにはせず、次の三層で読みやすくする。

1. **立体シルエット**: Observer、Piston、Comparator等を形で区別する
2. **機能記号**: facing矢印、Observerの検知/出力面、接続線を示す
3. **状態表示**: emissive、輪郭、0〜15数値、badgeを使う

powered状態、向き、選択状態は色だけで表現しない。近景では形状、遠景では接続・状態overlayを優先する。

Minecraftのmodel/textureを配布物へ含めない。ユーザー所有のclient JARからの抽出・cacheは公式に明示許諾された方式ではないため、MVPから外し、公開前の法務確認が済んだ場合だけ任意機能として検討する。先に扱うなら、ユーザーが権利を確認したresource packを明示選択する方式とし、projectへの埋込みやexportは行わない。

公開物にはMinecraft Usage Guidelinesが求める非公式製品の表示を置く。Nucleationのblock databaseにはvanilla texture由来のcolor cacheが含まれるため、公開配布前にbuild成果物を監査し、利用可否を確認する。

## 6. シミュレーション仕様

### 6.1 状態分離

- **Edit Document**: 保存・undoの対象となる設計の正本
- **Simulation Instance**: start時にEdit Documentから作るsnapshot

simulation結果はSimulation Instanceにだけ適用する。MVPでは「現在の実行状態を設計へ適用」は作らない。編集するとsimulationを停止してsnapshotを破棄する。

### 6.2 Nucleationの利用

Nucleationはschematic I/Oと`mc-tick`に利用する。初期実装では一つの具体実装を`simulation.rs`へ閉じ込め、独自`SimulationBackend` trait、fast backend、composite backendを作らない。

2026-08-23時点では、Nucleation v0.10.14 tagの`Cargo.toml`に`mc-tick` featureがある一方、crates.io版 0.10.14 の公開feature一覧には`mc-tick`と`simulation`がなく、`mc-tick` crate自体も公開されていない。このため、release commitをGit `rev`で固定し、`bridge` + `mc-tick`を有効にする。`master`追従やcrates.io版への置換は行わない。

`TickSimulation`はDiplomat bridge中心のAPIで、Rust向けの安定したidiomatic APIとはみなさない。Gate 0のcompile/API probeを必須とし、Nucleation内部型とJSON bridgeの変換は`simulation.rs`から外へ漏らさない。後置配置するBlockStateは`extra_states`へ登録し、登録漏れが黙ったno-opにならないよう配置後のstateを照合する。

Nucleationのsettle mode名は以下を使用する。原案の`Raw`は現行公開文書と一致しないため、`Quiet`へ訂正する。

| Mode | 用途 |
|---|---|
| InWorld | 保存済みworldの状態を信頼し、placement/settleを行わない。既定 |
| Placement | vanilla相当のplacement passと順序付きsettleを行う |
| Quiet | `onPlace`を行うが、update stormとsettleを発生させない |

### 6.3 対応表示

ユーザー向けの状態は三つにする。

| 状態 | 意味 |
|---|---|
| 検証済み | 固定したengine/versionのfixtureで期待結果を確認済み |
| 制限あり | 実行可能だが既知の制約を表示する |
| 非対応 | 表示・保持のみ、またはsimulationを開始できない |

非対応blockを含む場合は、block ID、座標、理由、可能なら回避策を示す。装飾用の静的blockと、結果を変え得る未対応mechanicは区別する。

各実行結果へ次を記録する。

- Minecraft対象version
- DataVersion
- Nucleation revision / engine ID
- settle mode
- seed
- world origin

MVPの対象はMinecraft Java 26.2 / DataVersion 4903だけとする。Nucleationの既知の制約もCapability表示へ反映する。現行文書では、mob AI/player physicsなし、item stackを64として扱う制約、一部entityが測定済みhitboxのみであること等が明記されている。

## 7. 最小アーキテクチャ

### 7.1 データの流れ

```text
Palette / Input ──edit transaction──▶ Edit Document
                                          │
                         ┌────────────────┴───────────────┐
                         ▼                                ▼ snapshot
                   Bevy projection                 TickSimulation
                         ▲                                │
                         └──────── block changes ─────────┘
```

Edit Documentを正本とし、Bevy entityを保存データの正本にしない。Nucleationの`UniversalSchematic`をDocument内で利用し、独自chunk/palette worldは性能計測で必要になるまで作らない。

編集履歴は、変更セルの`before/after`を持つtransactionで実装する。実装が一つしかない`EditCommand` traitや、block種別ごとのcommand classは作らない。

### 7.2 構成

MVPはCargo workspaceや多数のcrateへ分割せず、一つのpackageにする。

```text
src/
├── main.rs        # 起動のみ
├── lib.rs         # integration testから利用する入口
├── app.rs         # Bevy / egui / input / projection
├── brewing.rs     # 限定醸造の決定論的状態機械
├── document.rs    # Edit Document / transaction / file I/O
└── simulation.rs  # Nucleationへの薄い接続
```

headless CLIや二つ目のbackendが実際に必要になった時点で、初めてcrate分割またはtraitを検討する。

### 7.3 初期dependency候補

- Rust `=1.98.0`を`rust-toolchain.toml`で固定
- Bevy `=0.19.1`
- `bevy_egui = =0.42.0`
- Nucleationはrelease commit `04a4753fe73167888eb6372fd23678d818d22b56`をGit `rev`で固定し、`default-features = false`、`features = ["bridge", "mc-tick"]`でcompile probeする

Nucleationの`meshing`/`rendering` featureは、AGPL-3.0-onlyのSchematic-Mesherを取り込むため有効にしない。描画はBevyのwindow、3D、input、pickingを使い、Tauri、Tokio、独立したwinit/wgpuを追加しない。最初は可視blockごとの単純entityでよく、chunking、greedy meshing、instancingは計測で不足した場合だけ追加する。

現行規模ではsimulation stepを同期実行してもUI停止が観測されないため、worker threadは導入していない。実利用fixtureで停止が観測された処理だけをBevy task poolまたは一つの`std::thread`へ移す。

## 8. ファイル・安全性

### 8.1 MVPの保存

- 正式保存: `.litematic`
- 一時保存: 同一内容のrecovery fileをユーザーデータ領域へ一つ保持
- 書込: temporary fileへ完了後、atomic replaceする
- 読込: 完全に検証した新Documentを作ってから現在のDocumentと交換する

独自project形式、schema migration、世代autosaveは、probeやtest scenario等を保存する必要が生じてから追加する。

MVPの編集・保存保証はsingle-region `.litematic`に限定する。multi-regionはview-onlyまたは明示的に拒否し、黙ってmergeしない。round-tripはbyte一致ではなくBlockState/NBTの意味的一致とloss reportで判定する。

### 8.2 未信頼入力

`.litematic`と将来のresource packを未信頼入力として扱う。

- file size、world volume、non-air block数、entity数、NBT depthに上限を持つ
- 上限値は一箇所の定数とし、エラーへ実値と上限を示す
- path traversalを許可しない
- parse失敗やpanicで現在のDocumentを変更しない
- 保持できないunknown block/NBTを黙ってairへ変換しない

## 9. 品質基準

### 9.1 最小の自動確認

MVPでは独自scenario DSL、GUI test editor、JUnit出力を作らない。通常のRust unit testで次を確認する。

1. button操作からtickを進め、fixtureのPiston/lamp状態が期待値になる
2. supported BlockStateの `.litematic` round-tripが一致する
3. 複数セルtransactionのundo/redoとsave失敗時の非破壊性
4. container inventory、dispenser / water、限定醸造、potion NBTのround-trip

Nucleation自体が持つMinecraft captureとのconformanceを再実装せず、RedForge側では固定revisionとの接続部分を検証する。

### 9.2 性能

最適化前に、開発機のCPU/GPU、fixture、build profileを記録する。初期目安は以下とする。

- 5,000 non-air blockのIsometric表示で30 fps以上
- 単一配置は100 ms以内、通常は次frameに視覚反映
- simulation stepでUI停止が目立つ場合だけworker化する

128³、60 fps、全world remesh禁止等は、実利用fixtureによる計測後の目標とする。

### 9.3 アクセシビリティ

- powered、facing、selectionを色だけで表さない
- UI scaling
- key binding再設定はMVP後でもよいが、既定操作は画面内で確認可能にする
- tooltipへblock IDと主要BlockStateを表示する

## 10. 原案からの主要変更

| 原案 | 改訂 |
|---|---|
| Relay Forge | リポジトリに合わせRedForge。正式brandingは延期 |
| Build / Inspect / Logic / Splitの4 View | 一つのViewport＋Top/Isometric/Orbit＋overlay |
| 3Dでも編集 | MVPのOrbitは確認中心 |
| `.litematic`と`.schem`がMUST | MVPは`.litematic`のみ |
| 独自`.relay` ZIP | 保存すべき独自情報が生じるまで作らない |
| 約11 crate | 一つのpackage、三つの主要module |
| custom `DesignWorld` / chunk palette | `UniversalSchematic`をDocument内で再利用 |
| `SimulationBackend` traitと複数backend | Nucleation具体実装一つ |
| 独自RON test scenario | 通常のintegration test。将来はNucleation既存scenarioを再利用候補とする |
| 最低100種類のblock | 二つの代表回路を作れる最小集合 |
| Milestone 0〜3がMVP | Gate 0〜2。inventory/waterはBeta |
| pressure plateはMVP、entity接触は後期 | 同じMechanism Betaへ揃える |
| 醸造台を優先詳細設計 | 当初は延期し、ユーザー指定後に限定拡張として前倒し実装 |
| `Raw` settle mode | 現行Nucleation文書に合わせ`Quiet` |
| 対象Minecraft versionが未確定 | MVPはJava 26.2 / DataVersion 4903へ固定 |
| macOS/Linuxを同時対象 | Windows 11 x64から開始 |
| unlimitedに近いundo | 直近50操作以上 |
| Minecraft asset local importerを早期実装 | 独自Technical Themeを先行 |

## 11. 未確定事項と判断時期

| 未確定事項 | 決める時期 |
|---|---|
| Nucleation v0.10.14のtick bridgeがRustから実用可能か | Gate 0で実用可能と確認済み |
| Minecraft version更新 | Redstone MVP後、version別fixtureを用意できる時 |
| 1 block = 1 entityで足りる規模 | Gate 1の性能計測後 |
| Orbitでの自由配置が必要か | Design Alphaのユーザーテスト後 |
| `.schem`の優先度 | `.litematic`利用者から要望が出た時 |
| 独自project形式 | probe/scenario等を保存する必要が出た時 |
| 次のmechanic | Redstone MVPの利用シナリオを確認後 |
| 正式名称 | 公開配布前 |

## 12. 参考資料

- [Nucleation v0.10.14 release / MIT license](https://github.com/Schem-at/Nucleation/releases/tag/v0.10.14)
- [Nucleation v0.10.14 Cargo features](https://github.com/Schem-at/Nucleation/blob/v0.10.14/Cargo.toml)
- [Nucleation 0.10.14 crates.io package](https://crates.io/crates/nucleation/0.10.14)
- [Nucleation mc-tick Rust integration notes](https://github.com/Schem-at/Nucleation/blob/v0.10.14/crates/mc-tick/README.md)
- [Nucleation Tick simulation](https://github.com/Schem-at/Nucleation/blob/v0.10.14/docs/features/tick-simulation.md)
- [Nucleation Tick mechanics and limits](https://github.com/Schem-at/Nucleation/blob/v0.10.14/docs/features/tick-simulation-mechanics.md)
- [Nucleation Formats and I/O](https://github.com/Schem-at/Nucleation/blob/v0.10.14/docs/features/formats-and-io.md)
- [Nucleation Block database](https://github.com/Schem-at/Nucleation/blob/v0.10.14/docs/features/block-database.md)
- [Nucleation fluid limitations](https://github.com/Schem-at/Nucleation/blob/v0.10.14/crates/mc-tick/src/fluid.rs#L28-L36)
- [Bevy 0.19.1 release](https://github.com/bevyengine/bevy/releases/tag/v0.19.1)
- [bevy_egui 0.42 compatibility table](https://github.com/vladbat00/bevy_egui/blob/v0.42.0/README.md#bevy-support-table)
- [Rust 1.98.0 release](https://blog.rust-lang.org/2026/08/20/Rust-1.98.0/)
- [Shapez 2 official Steam page](https://store.steampowered.com/app/2162800/shapez_2/)
- [Minecraft Usage Guidelines](https://www.minecraft.net/usage-guidelines)
- [Minecraft EULA](https://www.minecraft.net/eula)
- [Schematic-Mesher AGPL-3.0 license](https://github.com/Schem-at/Schematic-Mesher/blob/main/LICENSE)
