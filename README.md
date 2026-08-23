# RedForge

Minecraft Java Edition向けの、ローカル動作するレッドストーン回路CAD・tickシミュレーターです。Rust / Bevyで実装し、回路データとtick実行には固定revisionのNucleationを利用しています。

現在はWindows 11 x64向けTechnical Previewです。Top / Isometricで素早く配置し、Orbitで立体確認しながら、設計を`.litematic`として保存できます。

## 現在できること

- Minecraftクライアントの実textureを使ったTop / Isometric / Orbit表示、active Y layer編集
- 32×32のflat-world床、1 block単位のgrid、影のないunlit表示
- Minecraft inventory icon / 代表textureのPalette grid、hover時のblock名表示
- blockの直線drag配置・削除、box選択、copy / paste、undo / redo
- redstone wire、torch、repeater、comparator、lamp、lever、button、Observer、Piston等のtick実行
- hopper、barrel、dispenser、dropperのinventory編集
- dispenserによるwater配置、pressure plate、静的farmland / wheatの表示・保存
- `.litematic`の新規作成・読込・atomic save・recovery save
- 醸造台の5 slot inventory、燃料、400 tick進行、3段階の最小レシピ、比較器出力

対応外・近似動作はInspectorのDiagnosticsに座標付きで表示します。

## 起動

必要なものはGitとRustupです。Rust 1.98.0は`rust-toolchain.toml`に固定されています。

```powershell
cd C:\Users\aod\Documents\GitHub\RedForge
cargo run --release
```

初回だけRust toolchainとGit依存関係の取得、releaseビルドに時間がかかります。開発中は`cargo run`でも起動できます。

Minecraftの見た目は、端末に導入済みのJava Edition client JARから起動時に読み込みます。公式Launcher、Prism Launcher、MultiMCを自動検出し、読み込めない場合は簡易色にfallbackします。任意のJARを指定する場合は、起動前に`REDFORGE_MINECRAFT_JAR`を設定してください。

## 基本操作

| 操作 | 入力 |
|---|---|
| block選択 | 左のPalette |
| 配置 / 連続配置 | 左click / drag |
| 削除 / 連続削除 | 右click / drag |
| pan / Orbit回転 | middle drag |
| zoom | wheel |
| 新規 / 開く / 保存 | `Ctrl+N` / `Ctrl+O` / `Ctrl+S` |
| 名前を付けて保存 | `Ctrl+Shift+S` |
| カメラ切替 | View menu、または`1` / `2` / `3` |
| カメラ90°回転 | `Q` / `E` |
| Y layer移動 | `[` / `]` |
| 配置blockの向き回転 | `R` |
| Paint / Select切替 | `V` |
| box選択 | Selectで始点・終点を左click |
| copy / paste | `Ctrl+C` / `Ctrl+V` |
| 選択範囲を削除 | `Delete` |
| undo / redo | `Ctrl+Z` / `Ctrl+Shift+Z` |
| 選択へfocus | `F` |
| simulation start / run / pause | Inspectorのボタン、または`Space` |
| 1 tick step | InspectorのStep、または`.` |

すべての編集・カメラ・simulation操作はWindowsのFile / Edit / View / Simulation menuからも実行できます。開く・保存はWindows標準ダイアログで`.litematic`を選びます。

## 醸造台を使う

1. Paletteの`Brewing Stand`を配置し、選択します。
2. Inspectorの`Inventory`を開き、`水→奇妙`、`奇妙→俊敏`、`俊敏→延長`のいずれかを選びます。
3. `Inventoryを適用`を押します。瓶のBlockStateとポーションNBTも保存対象になります。
4. `Start`後に`Run`または`Step`で進めます。通常速度なら400 tick = 20秒、100 ticks/secなら約4秒です。
5. 進捗、燃料、瓶の占有、比較器出力、醸造イベントをInspectorで確認できます。

実装しているレシピは、水入り瓶 + ネザーウォート → 奇妙なポーション、奇妙 + 砂糖 → 俊敏、俊敏 + レッドストーン → 効果時間延長です。燃料はブレイズパウダー1個で20回分、醸造開始時に1回分を消費します。

## 既知の制限

- Nucleationが醸造台block entityを未対応としているため、醸造はRedForge内の決定論的拡張です。比較器値は計算・表示しますが、醸造中のinventory変化をNucleation側の下流回路へ動的伝播する処理は未対応です。
- water flow / waterlogged形状にはNucleation由来の近似があります。
- farmland / wheatは表示・保存のみで、random tickや成長は実行しません。モブAIも対象外です。
- `.litematic`編集はsingle-region限定です。multi-regionは黙って統合せず、読込を拒否します。
- Minecraft資産は同梱・再配布せず、導入済みclient JARからメモリ上に読み込みます。block modelから動的生成されるinventory iconは完成PNGがないため、現時点では実block textureの代表面を表示します。
- 複雑なblock modelは保存・simulation用のBlockStateを維持しつつ、viewportでは実texture付きの簡略形状で表示します。
- simulation結果は設計へ書き戻しません。編集を始めるとsimulation snapshotを破棄します。

## 開発時の確認

```powershell
cargo fmt --all -- --check
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo build --release
```

設計判断、対応範囲、Nucleation revision等の詳細は[要件定義・基本設計](docs/requirements.md)を参照してください。
