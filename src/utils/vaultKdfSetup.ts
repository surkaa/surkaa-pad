export const KIB_PER_MIB = 1024;

export type VaultMemoryOption = {
  label: string;
  value: number;
};

export function defaultNewVaultMemoryCost(isDevelopment = import.meta.env.DEV): number {
  return isDevelopment ? KIB_PER_MIB : 256 * KIB_PER_MIB;
}

export function newVaultMemoryOptions(isDevelopment = import.meta.env.DEV): VaultMemoryOption[] {
  return [
    ...(isDevelopment ? [{label: '1 MiB · 调试', value: KIB_PER_MIB}] : []),
    {label: '64 MiB · 兼容', value: 64 * KIB_PER_MIB},
    {label: '128 MiB · 均衡', value: 128 * KIB_PER_MIB},
    {label: '256 MiB · 更强', value: 256 * KIB_PER_MIB},
  ];
}
