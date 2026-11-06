# ワークスペースと設定

ワークスペースは普通のフォルダで、バックアップしたり git に入れたりできます。中身は次のとおりです：

```
<工作区>/
├─ globalcontact.toml   工作区配置：阶段、字段、摘要提示词、每日上限
├─ playbooks/           分析目的和业务背景
├─ lists/               外联名单
├─ suppression.txt      不再联系的地址
├─ preferences.md       你的写作偏好，所有起草和分析都会遵循
├─ contacts.ktable      联系人、渠道、来源身份、往来记录（可用表格编辑器打开）
├─ people/              每人一份档案
├─ memory/              Agent 记录：每人一份，按时间追加
├─ drafts/              草稿
├─ intake/              记录的资料和反馈
├─ mail/                同步的邮件
├─ sources/             数据源事件的证据文件
└─ notes/               Agent 写的报告等
```

| パス | 内容 |
|---|---|
| `globalcontact.toml` | ワークスペースの設定：ステージ、フィールド、要約のプロンプト、1 日の上限 |
| `playbooks/` | 分析の目的とビジネス背景 |
| `lists/` | アウトリーチのリスト |
| `suppression.txt` | 連絡しないアドレス |
| `preferences.md` | あなたの書き方の好み。すべての下書きと分析が従います |
| `contacts.ktable` | 連絡先、チャネル、ソースの ID、やり取り（表エディタで開けます） |
| `people/` | 一人につき 1 つの人物メモ |
| `memory/` | Agent メモリ：一人につき 1 つ、時系列で追記 |
| `drafts/` | 下書き |
| `intake/` | 記録した資料とフィードバック |
| `mail/` | 同期したメール |
| `sources/` | データソースのイベントの証拠ファイル |
| `notes/` | Agent が書いたレポートなど |

メールのパスワード、ソースのトークン、ターミナルの記録、同期の進捗はこのコンピュータのアプリデータフォルダに保存され、**ワークスペースには含まれません**。

## globalcontact.toml

```toml
stages = [
  { label = "待联系", role = "open" },
  { label = "已发", role = "waiting" },
  { label = "已回复", role = "active" },
  { label = "进行中", role = "active" },
  { label = "搁置", role = "closed" },
  { label = "不再联系", role = "suppressed" },
]

[outreach]
daily_limit = 20

[contacts]
campaigns = []
summary_prompt = "根据这个联系人的资料，总结……"
```

- **ステージ名は自由に変えられます**。アプリが見るのは `role` だけです：
  - `open`：未連絡；
  - `waiting`：送信済みで返信待ち；
  - `active`：返信あり、または進行中；
  - `closed`：一時停止または解決済み；
  - `suppressed`：連絡しない。
- 少なくとも open、waiting、active の 3 つのステージが必要です。新しい連絡先は最初の open ステージに入り、メッセージを送ると最初の waiting ステージに、返信を受け取ると最初の active ステージに変わります。
- `[[fields]]` は表の作成時に追加するフィールドを定義します（新しいワークスペースの作成時のみ有効）。
- 変更後はワークスペースを開き直すと反映されます。形式に誤りがある場合は、開くときに具体的な理由が表示されます。

## 別のワークスペースを開く

左上のワークスペース名 → ワークスペースを開く… をクリックします。1 つのワークスペースは同時に 1 つの OutreachDex ウィンドウでしか開けません。
