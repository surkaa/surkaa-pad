export function masterPasswordConfirmationError(
  password: string,
  confirmation: string,
): string | null {
  if (!password) return '主密码不能为空'
  if (!confirmation) return '请再次输入主密码'
  if (password !== confirmation) return '两次输入的主密码不一致'
  return null
}
