// WorkBuddy 适配入口：设 agent 标识 → 调共享 CC-兼容 ingest。
// 由 ~/.workbuddy/settings.json 的 hooks 以 `node .../adapters/workbuddy/ingest.mjs` 调用。
import { run } from '../_shared/cc-ingest.mjs';
run('workbuddy');
