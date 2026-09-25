#!/usr/bin/env node
/**
 * PreToolUse 钩子：发布到 crates.io 前必须用户当次确认。
 *
 * 为什么：crates.io 的版本永久不可撤回（只能 yank，不能删、不能覆盖）。
 * 0.1.0 漏配 docs.rs 的 feature，就只能补发 0.1.1 —— 发错的版本永远留在记录里。
 *
 * 为什么是「询问」而不是「拦截」：本 crate 就是要发布的，拦死会逼人换 shell 绕过去，
 * 那等于没有护栏。询问在跳过权限模式下也会弹出，用户点同意才执行。
 *
 * 放行：`cargo publish --dry-run`（只打包验证，不上传）。
 * 覆盖：Bash 与 PowerShell 两个工具；命令里任意位置出现 cargo publish 都算
 * （包括 `cmd /c "cargo publish ..."` 这类包了一层的写法）。
 */
let raw = '';
process.stdin.on('data', (c) => (raw += c));
process.stdin.on('end', () => {
  let command = '';
  try {
    command = String(JSON.parse(raw)?.tool_input?.command ?? '');
  } catch {
    process.exit(0); // 解析不了就不插手，交给默认权限流程
  }

  const isPublish = /\bcargo(\.exe)?\s+(\+\S+\s+)?publish\b/i.test(command);
  const isDryRun = /--dry-run\b/.test(command);
  if (!isPublish || isDryRun) process.exit(0);

  const out = {
    hookSpecificOutput: {
      hookEventName: 'PreToolUse',
      permissionDecision: 'ask',
      permissionDecisionReason:
        '即将发布到 crates.io（版本永久不可撤回）。确认前请核对：' +
        '版本号与 CHANGELOG 已更新、发版闸门全部通过、GitHub CI（含 MSRV）已绿。' +
        '流程见技能 crate-release。',
    },
  };
  process.stdout.write(JSON.stringify(out));
  process.exit(0);
});
