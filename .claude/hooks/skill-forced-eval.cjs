#!/usr/bin/env node
/**
 * UserPromptSubmit Hook - 强制技能评估（ai-profile）
 *
 * 为什么要有：技能靠描述自动匹配时，最关键的 change-impact（改完还要连带改什么）最容易被跳过 ——
 * 用户只会说「修个 bug」「加个模型」，不会说「查一下连带更新」。每次提问都把技能清单摆出来，
 * 让模型先评估再动手。
 *
 * 与 sigil / Tauri 框架版的区别：技能清单**运行时从 .claude/skills/*\/SKILL.md 读取**
 * （name + 「触发词：」那一行），不在本文件里写死 —— 加 / 改技能不用回来改钩子，也就不会漂移。
 *
 * 任何异常一律静默退出（exit 0 且不输出）：钩子出错不能挡住用户提问。
 */

const fs = require('fs');
const path = require('path');

let input;
try {
  input = JSON.parse(fs.readFileSync(0, 'utf8'));
} catch {
  process.exit(0);
}

const prompt = (input.prompt || '').trim();

// 恢复会话 / 压缩后续接：注入清单只会白占上下文，甚至引发溢出循环
const skipPatterns = [
  'continued from a previous conversation',
  'ran out of context',
  'No code restore',
  'Conversation compacted',
  'commands restored',
  'context window',
  'session is being continued',
];
if (skipPatterns.some((p) => prompt.toLowerCase().includes(p.toLowerCase()))) {
  process.exit(0);
}

// 斜杠命令自带流程，不需要再评估
if (/^\/[^\/\s]+$/.test(prompt.split(/\s/)[0] || '')) {
  process.exit(0);
}

/** 读出每个技能的 name 与触发词；读不到触发词时只列名字，读不到目录则放弃注入 */
function loadSkills() {
  const root = process.env.CLAUDE_PROJECT_DIR || path.resolve(__dirname, '..', '..');
  const dir = path.join(root, '.claude', 'skills');
  const out = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    const file = path.join(dir, entry.name, 'SKILL.md');
    if (!fs.existsSync(file)) continue;
    const text = fs.readFileSync(file, 'utf8');
    const name = (text.match(/^name:\s*(.+)$/m) || [])[1] || entry.name;
    const triggers = (text.match(/^\s*触发词[:：]\s*(.+)$/m) || [])[1] || '';
    out.push({ name: name.trim(), triggers: triggers.trim() });
  }
  return out.sort((a, b) => a.name.localeCompare(b.name));
}

let skills;
try {
  skills = loadSkills();
} catch {
  process.exit(0);
}
if (skills.length === 0) process.exit(0);

const list = skills
  .map((s) => (s.triggers ? `- ${s.name}: ${s.triggers}` : `- ${s.name}`))
  .join('\n');

const instructions = `## 强制技能激活流程（必须执行）

### 步骤 1 - 评估（必须在响应中明确展示）

针对用户问题，列出匹配的技能：\`技能名: 理由\`，无匹配则写"无匹配技能"

可用技能（来自 .claude/skills/）：

${list}

🔴 **只要这一轮会改 crate 代码、预置、spec/ 或公开 API，change-impact 一律算匹配** ——
它管「改完还要连带改什么」（生成物、文档站、多语言规范、README 数字、下游），漏了没有任何报错。

### 步骤 2 - 激活
对每个匹配的技能逐个激活：一次激活一个，等它返回后再激活下一个（不要一次性批量激活）。无匹配则跳过本步。

### 步骤 3 - 实现
所有匹配技能激活完成后，再开始动手实现。

要点：先评估 → 再激活 → 后实现；不要跳过激活直接实现，也不要漏掉匹配的技能。`;

console.log(instructions);
process.exit(0);
