import type { InjectionKey, Ref } from "vue";
import type { PortType } from "../../domain/workflow";

/** 拖线连接时的上下文：BaseNode 据此把不兼容的目标端口置灰 */
export interface ConnectingContext {
  state: Ref<{ nodeId: string; port: string; type: PortType } | null>;
  canTarget: (nodeId: string, port: string) => boolean;
}

export const CONNECTING_KEY: InjectionKey<ConnectingContext> = Symbol("wf-connecting");
