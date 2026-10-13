export const KIB_PER_MIB = 1024;

// 成本影响的是主密码派生与猜测开销，不改变 AES-256 的密钥长度。
export const VAULT_KDF_COST_HINT = '内存成本会影响暴力破解主密码的耗时；较高成本通常会增加每次猜测的时间和内存开销，也会增加本机解锁耗时和内存占用，不能替代强主密码。';
export const DEV_VAULT_KDF_COST_HINT = '1 MiB 仅供开发测试，抗暴力破解能力较弱，不建议用于正式数据。';

export function isDevelopmentEdition(identifier: string, name: string, frontendDevelopment: boolean): boolean {
  // Windows Dev 安装包有独立标识；Android Dev 沿用正式标识，因此还需检查产品名。
  // 不根据当前 Vault 的内存成本判断版本，正式版仍可能导入已有的低成本配置。
  return frontendDevelopment || identifier === 'cn.surkaa.pad.dev' || name === 'SurKaa Pad (Dev)';
}

export type VaultMemoryOption = {
  label: string;
  value: number;
};

export function defaultNewVaultMemoryCost(isDevelopment = import.meta.env.DEV): number {
  return isDevelopment ? KIB_PER_MIB : 256 * KIB_PER_MIB;
}

export function newVaultMemoryOptions(isDevelopment = import.meta.env.DEV): VaultMemoryOption[] {
  return [
    ...(isDevelopment ? [{label: '1 MiB · 开发测试', value: KIB_PER_MIB}] : []),
    {label: '64 MiB · 兼容', value: 64 * KIB_PER_MIB},
    {label: '128 MiB · 均衡', value: 128 * KIB_PER_MIB},
    {label: '256 MiB · 更强', value: 256 * KIB_PER_MIB},
  ];
}
