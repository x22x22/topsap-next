import { QueryClient } from "@tanstack/react-query";

export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: 1,
      refetchOnWindowFocus: false,
      staleTime: 30_000,
    },
    mutations: {
      retry: 0,
    },
  },
});

/** 稳定的 queryKey，新增前先查重 */
export const queryKeys = {
  appInfo: ["app-info"] as const,
  settings: ["settings"] as const,
  proxyBypass: ["proxy-bypass-status"] as const,
  routePolicy: ["route-policy"] as const,
  residue: ["residue"] as const,
  snapshot: ["snapshot"] as const,
};
