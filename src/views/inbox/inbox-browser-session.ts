/** 收件箱和工作台各有自己的浏览器组件，但原生 webview 只有一个。 */
export const browserSession: {
  owner: symbol | null;
  chain: Promise<void>;
  loadedUrl: string;
} = {
  owner: null,
  chain: Promise.resolve(),
  loadedUrl: "",
};
