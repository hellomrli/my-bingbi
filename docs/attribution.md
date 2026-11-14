# 知识包来源与许可

[返回文档入口](index.md)

公文知识底座参考了多个开源 Skill 项目，互补覆盖格式、语言、量化节奏和工程流程：

- [KaguraNanaga/official-document-writing-skill](https://github.com/KaguraNanaga/official-document-writing-skill)（MIT）— GB/T 9704-2012 格式详解、文种模板、质量检查清单
- [Liuxiangjian-ai/official-document-skill](https://github.com/Liuxiangjian-ai/official-document-skill)（MIT）— 文种决策、人民日报表达规律、降 AI 味与 humanizer 审稿
- [larashero3-dotcom/lieflat-gongwen](https://github.com/larashero3-dotcom/lieflat-gongwen)（PolyForm Noncommercial 1.0.0）— 真实语料的句长、顿号、标题、力度词与收尾分布；本仓库只吸收统计口径与写作原则并本地重写
- [Rimagination/gongwen-draft](https://github.com/Rimagination/gongwen-draft)（仓库未附许可证）— 素材台账、来源优先级、引用核验、标点与易混词审核方法；本仓库只吸收方法并本地重写

本项目的工作是把其中可机器判定的部分从提示词转化为配置常量与校验规则，其余保留为按需检索的知识包。转换对照见 [`docs/PLAN.md` 附录](PLAN.md)。上游非商用/未声明许可的内容若用于商业发行，请先自行评估适用性。

具体声明保留在 [GB9704 包](../packs/gb9704-format/THIRD_PARTY.md) 和 [语言方法包](../packs/renmin-style/THIRD_PARTY.md)。运行时 SKILL.md、参考材料和校验规则属于应用资源，本次文档整理保留它们的原内容。
