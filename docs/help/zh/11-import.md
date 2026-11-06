# 导入联系人

## 从 CSV / TSV 导入

1. 在「人」页点「导入」，选择文件。Excel 请先「另存为 CSV UTF-8」。
2. 应用会预览前几行，并根据列名自动对应字段。中英文列名都能识别，例如 姓名 / Name、邮箱 / Email、公司 / Affiliation、备注 / Notes。
3. 在每列上方调整对应字段，或选择「不导入」。必须有一列对应「Name」。
4. 点「导入」。

导入规则：

- 先按**邮箱**、再按**姓名**匹配已有联系人，匹配到就更新非空的字段，否则新建。
- 邮箱、主页、X、Instagram 会同时加入联系渠道。
- WhatsApp、TikTok、Facebook（列名写 Messenger 也可以）、LinkedIn 列只加入联系渠道，不占用联系人字段。
- 「阶段」列的值必须是工作区配置里的阶段名称，否则会被忽略并给出提示。

## 命令行导入

```sh
globalcontact import contacts.csv --workspace /path/to/workspace
```

目标工作区不能正在被桌面应用使用。新建工作区时可以加 `--template user-relationships` 等指定模板。

## 在表格中批量整理

「人」页的「表格编辑」会打开联系人表格，可以直接新增行、修改字段，适合批量整理。
