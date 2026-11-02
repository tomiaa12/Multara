/** 必须在 `import "monaco-editor"` 之前执行，才能盖住模块顶层的 localize()。 */
import { applyMonacoNls } from "./monaco-nls";

applyMonacoNls();
