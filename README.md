<div align="center">

# Rust Type-3 Clone Detector

トークン情報と抽象構文木情報を用いた  
Rust向けコードクローン検出プログラム

</div>

---

## 概要

本プログラムは，Rustプロジェクトから関数およびメソッドを抽出し，トークン情報と抽象構文木情報を用いてコードクローンを検出します。

```text
関数・メソッドの抽出
        ↓
トークン情報による候補対の抽出と判定
        ↓
候補対に対するASTの生成
        ├─ 部分木ハッシュによる判定
        └─ AST特徴ベクトルによる判定
```

## プログラム構成

| プログラム | 内容 |
|---|---|
| `crates/function_extractor` | Rustファイルから関数およびメソッドを抽出し，字句解析結果や行番号などを取得します |
| `scripts/token_filter.py` | N行ブロックの一致率とトークン重複度を用いて，クローン対とAST判定用の候補対に分類します |
| `crates/ast_builder` | 候補対に対応する関数およびメソッドからASTを生成します |
| `scripts/ast_hash_detection.py` | 部分木ハッシュに基づいてAT類似度とDice類似度を計算します |
| `scripts/ast_vector_detection.py` | AST特徴ベクトルに基づいてJaccard類似度を計算します |
| `scripts/run_pipeline.bat` | 各処理を順番に実行します |

## 一括実行

リポジトリのルートディレクトリで，次のコマンドを実行します。

```bat
scripts\run_pipeline.bat "<解析対象のRustプロジェクト>" "<出力先ディレクトリ>"
```

第1引数には解析対象のRustプロジェクト，第2引数には中間ファイルおよび検出結果の出力先を指定します。

`scripts`ディレクトリから実行する場合は，次のように指定します。

```bat
run_pipeline.bat "<解析対象のRustプロジェクト>" "<出力先ディレクトリ>"
```

## 個別実行

以下のコマンドは，リポジトリのルートディレクトリで実行します。

### 1. 関数およびメソッドの抽出

```bat
cargo run --release -p rust_extractor -- "<Rustプロジェクト>" "<出力先>\functions_rust.jsonl"
```

### 2. トークン情報による候補対の抽出と判定

```bat
py scripts\token_filter.py "<出力先>\functions_rust.jsonl" "<出力先>"
```

### 3. 候補対に対するASTの生成

```bat
cargo run --release -p ast_builder -- "<出力先>\ast_candidates.jsonl" "<Rustプロジェクト>" "<出力先>\rust_pairs_with_ast.jsonl"
```

### 4. 部分木ハッシュによる判定

```bat
py scripts\ast_hash_detection.py "<出力先>\rust_pairs_with_ast.jsonl" "<出力先>\rust_ast_hash.jsonl" all --threshold 0.65 --dice-threshold 0.70
```

### 5. AST特徴ベクトルによる判定

```bat
py scripts\ast_vector_detection.py "<出力先>\rust_pairs_with_ast.jsonl" "<出力先>\rust_ast_vector.jsonl" all --q 1 --threshold 0.75
```

`py`を使用できない場合は，`py`を`python`に置き換えてください。

## 主な出力ファイル

| ファイル | 内容 |
|---|---|
| `functions_rust.jsonl` | 抽出した関数およびメソッドの情報 |
| `direct_clones.jsonl` | トークン情報によりクローンと判定された対 |
| `ast_candidates.jsonl` | ASTによる判定に渡される候補対 |
| `rust_pairs_with_ast.jsonl` | AST情報を付与した候補対 |
| `rust_ast_hash.jsonl` | 部分木ハッシュによる判定結果 |
| `rust_ast_vector.jsonl` | AST特徴ベクトルによる判定結果 |

## パラメータ

トークン段階のパラメータは，`scripts/token_filter.py`の先頭付近で設定します。

```python
MIN_TOKENS = 50
N = 3
THETA1 = 0.15
THETA2 = 0.50
THETA3 = 0.70
THETA4 = 0.70
```

AST段階のパラメータは，`scripts/run_pipeline.bat`の先頭付近で設定します。

```bat
set "HASH_THRESHOLD=0.65"
set "HASH_DICE_THRESHOLD=0.70"
set "VECTOR_Q=1"
set "VECTOR_THRESHOLD=0.75"
```
