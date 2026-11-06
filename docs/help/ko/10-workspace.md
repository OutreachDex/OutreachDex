# 워크스페이스와 설정

워크스페이스는 평범한 폴더이며 백업하거나 git에 넣을 수 있습니다. 안에는 다음이 있습니다:

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

| 경로 | 내용 |
|---|---|
| `globalcontact.toml` | 워크스페이스 설정: 단계, 필드, 요약 프롬프트, 하루 한도 |
| `playbooks/` | 분석 목적과 비즈니스 배경 |
| `lists/` | 아웃리치 리스트 |
| `suppression.txt` | 연락 안 함 주소 |
| `preferences.md` | 내 글쓰기 선호. 모든 초안 작성과 분석이 따릅니다 |
| `contacts.ktable` | 연락처, 채널, 소스 ID, 활동 기록(표 편집기로 열 수 있음) |
| `people/` | 사람마다 하나의 인물 노트 |
| `memory/` | Agent 메모리: 사람마다 하나, 시간순으로 덧붙임 |
| `drafts/` | 초안 |
| `intake/` | 기록한 자료와 피드백 |
| `mail/` | 동기화한 메일 |
| `sources/` | 데이터 소스 이벤트의 증거 파일 |
| `notes/` | Agent가 쓴 보고서 등 |

메일 비밀번호, 소스 토큰, 터미널 기록, 동기화 진행 상황은 이 컴퓨터의 앱 데이터 폴더에 저장되며 **워크스페이스에는 없습니다**.

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

- **단계 이름은 마음대로 바꿀 수 있으며**, 앱은 `role`만 봅니다:
  - `open`: 연락 전;
  - `waiting`: 보냈고 답장 대기;
  - `active`: 답장했거나 진행 중;
  - `closed`: 보류 또는 해결됨;
  - `suppressed`: 연락 안 함.
- 최소한 open, waiting, active 세 가지 단계가 필요합니다. 새 연락처는 첫 번째 open 단계로 들어가고, 메시지를 보내면 첫 번째 waiting 단계로, 답장을 받으면 첫 번째 active 단계로 바뀝니다.
- `[[fields]]`는 표를 만들 때 추가할 필드를 정의합니다(새 워크스페이스를 만들 때만 적용).
- 수정한 뒤 워크스페이스를 다시 열면 적용됩니다. 형식이 잘못되면 열 때 구체적인 이유를 알려 줍니다.

## 다른 워크스페이스 열기

왼쪽 위 워크스페이스 이름 → 워크스페이스 열기…를 클릭합니다. 한 워크스페이스는 한 번에 하나의 OutreachDex 창에서만 열 수 있습니다.
