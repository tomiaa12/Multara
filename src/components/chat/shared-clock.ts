import { onBeforeUnmount, onMounted, readonly, ref } from "vue";

export const RELATIVE_TIME_TICK_MS = 30_000;

const now = ref(Date.now());
let subscribers = 0;
let timer = 0;

/** 所有消息页脚共用一个相对时间时钟，有订阅者时才计时。 */
export function useSharedNow() {
  onMounted(() => {
    subscribers += 1;
    if (timer) return;
    now.value = Date.now();
    timer = window.setInterval(() => {
      now.value = Date.now();
    }, RELATIVE_TIME_TICK_MS);
  });
  onBeforeUnmount(() => {
    subscribers -= 1;
    if (subscribers > 0 || !timer) return;
    window.clearInterval(timer);
    timer = 0;
  });
  return readonly(now);
}
