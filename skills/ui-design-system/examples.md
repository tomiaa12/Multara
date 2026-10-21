# Examples

## 正确：加一个按钮

用户：在设置页加「保存」。

```vue
<UiButton variant="primary" :disabled="busy" @click="save">
  <Save :size="16" />{{ busy ? "保存中…" : "保存设置" }}
</UiButton>
```

不要：

```vue
<button class="save-btn">保存</button>
```

```css
.save-btn {
  background: #6366f1;
  border-radius: 10px;
  padding: 10px 20px;
}
```

## 正确：工作台图标按钮

```vue
<UiButton size="icon" title="刷新" :disabled="loading" @click="reload">
  <RefreshCw :size="13" />
</UiButton>
```

工作台标题栏里再压到 22×22 时，沿用 `.explorer-title .ui-button` / `.scm-heading .ui-button`，不要新写一套。

## 正确：加一个弹窗

```vue
<div class="modal-backdrop" @click.self="open = false">
  <div class="modal" role="dialog" aria-modal="true">
    <h2>新建</h2>
    <label>名称<UiInput v-model="name" placeholder="例如：…" /></label>
    <div class="modal-actions">
      <UiButton @click="open = false">取消</UiButton>
      <UiButton variant="primary" :disabled="!name.trim()">创建</UiButton>
    </div>
  </div>
</div>
```

不要：自建 overlay、480px 圆角 20 的卡片、居中大按钮、渐变顶栏。

## 正确：空状态

页面：

```vue
<div class="empty-project">
  <p>还没有项目</p>
  <UiButton variant="primary"><Plus :size="16" /> 新建需求</UiButton>
</div>
```

工作台：

```vue
<div class="tree-empty">正在读取文件…</div>
```

## 正确：颜色 / 间距

```css
.my-row {
  display: flex;
  align-items: center;
  gap: var(--space-4);
  padding: var(--space-4) var(--space-6);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  background: var(--panel);
  color: var(--text);
}
.my-row:hover {
  border-color: var(--border-hover);
  background: var(--surface-hover);
}
```

不要：`padding: 13px 17px; border-radius: 10px; background: #1f2937;`

## 错误模式对照

| 错误 | 原因 | 改成 |
|---|---|---|
| `background: #6366f1` | 不是项目蓝 | `var(--blue)` |
| `border-radius: 16px` | 过大 | 卡片 9 / 按钮 6 / 对话框 11 |
| 新 `Toast.vue` | 无此模式 | `.save-success` 或 Dialog |
| 引入 Tailwind | 禁止新框架 | 现有 CSS |
| Lucide + 手写 SVG 混用 | 图标体系分裂 | 只用 Lucide / FileTypeIcon |
| 工作台主按钮 height 40 | 密度错误 | 28–34 或 22 icon |
| 按 `prototype/css/tokens.css` 改色 | 非生产 | `src/styles.css` |
| `StopAiDialog` 的 `.modal-card` | class 不存在 | 新对话框用 `.modal` |

## 用户指令应被理解成

「加一个按钮」=

> 在当前页面密度下，复用 `UiButton` 现有 variant/size，使用已有 Lucide 图标与 token。

「加一个弹窗」=

> 复用 `.modal-backdrop` + `.modal` + `modal-actions`，字段用 `UiInput`，按钮右对齐，主操作 primary 在最右。
