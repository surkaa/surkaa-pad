const timeEmojiMap = [
    '🕐',
    '🕑',
    '🕒',
    '🕓',
    '🕔',
    '🕕',
    '🕖',
    '🕗',
    '🕘',
    '🕙',
    '🕚',
    '🕛'
];

export function getCurEmoji(timestamp?: number): string {
    const date = timestamp ? new Date(timestamp) : new Date();
    const hour = date.getHours() % 12;
    return timeEmojiMap[hour];
}