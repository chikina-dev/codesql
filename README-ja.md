# codesql

`codesql`はAIフレンドリーでSQL風のコード検索ができるCLIツールです。
開発者やAIエージェント向けに、コードベースをインデックス化し、ファイルやコードの中身をSQLクエリを用いて爆速で検索することができます。
AIエージェントがよく何回もgrepやrgを繰り返すので、インデックスを作れば検索が早くなるのではと言う推測から作りました。

## ワークフロー

`codesql` を使うための基本的なワークフローは以下の通りです。

1. **ワークスペースの初期化**:
    ```sh
    codesql init
    ```
    ローカルデータベースのインデックスと設定を保存するための `.codesql` ディレクトリが作成されます（新しいリポジトリで作業を始める際は、必ず最初に実行してください）。

2. **インデックスの保存（更新）**:
    ```sh
    codesql save
    ```
    *注意: ファイルの追加・変更・削除を行った後は、インデックスを最新の状態に保つために必ずこのコマンドを実行してください。*

3. **SQLライクな構文での検索**:
    ```sh
    codesql search "SELECT path FROM files WHERE ext = 'rs'"
    ```

## 検索の構文

`search` クエリは、`files` という仮想的なベーステーブルを対象にします。

### 利用可能な `SELECT` フィールド
- `path`: ファイルの相対パスです。
- `line_no`: 一致した行の行番号（注意: 使う際は `WHERE` 句で `content` に対し `contains` または `regex` 関数を併用している必要があります）。
- `line`: 一致した行のコードテキストそのまま（注意: これも `WHERE` 句で `content` への `contains` や `regex` 関数の併用が必要です）。

### 利用可能な `WHERE` フィールド
- `path` (文字列): ファイルのパス。
- `ext` (文字列): ファイルの拡張子。
- `language` (文字列): 自動判別されたプログラミング言語（例: `'rust'`, `'typescript'`, `'javascript'`）。

### 利用可能な関数
- `contains(content, 'needle')`: ファイルの中身に `'needle'` という文字列が含まれていれば一致します。
- `regex(content, 'pattern')`: ファイルの中身が正規表現パターンの `'pattern'` にマッチすれば一致します。
- `glob(field, 'pattern')`: 指定したフィールドが特定のglobパターンにマッチするかをチェックします（例: `glob(path, 'src/**/*.rs')`）。
- `has_symbol(kind, name)`: ファイル内に特定の構文シンボルが存在するかを確認します。
  - `kind` は シンボルの種類（例: `'Function'`, `'Class'`, `'Struct'`など）を指定します。
  - `name` にはシンボル名を完全一致の文字列表記で渡せるほか、 `regex('...')` や `glob('...')` 構文のラップも使用できます。

## クエリの実行例

- **特定の拡張子 (Rust) のファイルをすべて検索:**
  ```sql
  SELECT path FROM files WHERE ext = 'rs'
  ```

- **TODOが含まれるファイルを検索し、ファイルパス・行番号・行テキストを上限10件で出力:**
  ```sql
  SELECT path, line_no, line FROM files WHERE contains(content, 'TODO') LIMIT 10
  ```

- **`main` という関数が定義されているRustファイルを検索:**
  ```sql
  SELECT path FROM files WHERE has_symbol('Function', 'main') AND ext = 'rs'
  ```

- **名前に "Manager" が含まれるクラスを正規表現で検索:**
  ```sql
  SELECT path FROM files WHERE has_symbol('Class', regex('.*Manager'))
  ```

## データベースのメンテナンス

大量のファイルを書き換えたりして検索速度が一時的に落ちた場合は、以下のコマンドでデータベースインデックスを最適化（圧縮等）してください:
```sh
codesql optimize
```
