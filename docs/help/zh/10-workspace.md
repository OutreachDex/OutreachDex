# 工作区与配置

工作区是一个普通文件夹，可以备份或放进 git。里面有：

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

邮箱密码、来源 token、终端记录和同步进度保存在本机的应用数据目录，**不在工作区里**。

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

- **阶段名称可以随意改**，应用只认 `role`：
  - `open`：未联系；
  - `waiting`：已发出等回复；
  - `active`：已回复或进行中；
  - `closed`：暂停或已解决；
  - `suppressed`：不再联系。
- 至少需要 open、waiting、active 三种阶段。新联系人进入第一个 open 阶段；发出消息后变为第一个 waiting 阶段，收到回复后变为第一个 active 阶段。
- `[[fields]]` 定义建表时追加的字段（只在新建工作区时生效）。
- 修改后重新打开工作区即可生效。如果格式有误，打开时会提示具体原因。

## 打开别的工作区

点左上角工作区名称 → 打开工作区…。同一个工作区同时只能被一个 OutreachDex 窗口打开。
