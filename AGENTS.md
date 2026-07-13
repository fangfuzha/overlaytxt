# OverlayTxt Agent Rules

## Skill 体系

skill 基础目录为 `.agents/skills/`，每个 skill 是一个子目录：

```
.agents/skills/
  <skill-name>/
    SKILL.md           -- skill 主文件
    references/         -- 专项参考文件（每个坑位/知识点一个文件）
```

**踩坑 skill**（`directcomposition-overlay-pitfalls`）：所有 DirectComposition 透明覆盖层开发中遇到的坑位汇总在该 skill 的 `references/` 下，每坑一文件。`SKILL.md` 建立索引（引用 + 简要解释），方便新人快速定位。

遇到新坑解决后，在 reference 下新建文件记录，并在 SKILL.md 索引中添加条目。