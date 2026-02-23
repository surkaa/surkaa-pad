import {DiarySummary} from "../bindings.ts";
import {formatBytes, formatTimestamp} from "./format.ts";
import {QVueGlobals} from "quasar";

export function showDiaryDetail($q: QVueGlobals, diary?: DiarySummary) {
    if (!diary) {
        $q.notify({type: 'negative', message: '无法获取日记详情'});
        return;
    }
    const {title, created, updated, attachments} = diary;
    let message = '';
    message += `创建时间：${formatTimestamp(created)}<br>`;
    message += `更新时间：${formatTimestamp(updated)}<br>`;
    message += `附件数量：${attachments.length}<br>`;
    // 展示附件表格
    message += '附件列表：<br>';
    message += '<table style="width: 100%; border-collapse: collapse;">';
    message += '<tr>';
    message += '<th style="border: 1px solid #ccc; text-align: center;">是否加密</th>';
    message += '<th style="border: 1px solid #ccc; text-align: center;">类型</th>';
    message += '<th style="border: 1px solid #ccc; text-align: center;">大小</th>';
    message += '</tr>';
    for (const att of attachments) {
        message += `<tr>`;
        message += `<td style="border: 1px solid #ccc; text-align: center;">${att.encrypted ? '是' : '否'}</td>`;
        message += `<td style="border: 1px solid #ccc; text-align: center;">${att.mimetype}</td>`;
        message += `<td style="border: 1px solid #ccc; text-align: center;">${formatBytes(att.size)}</td>`;
        message += `</tr>`;
    }
    $q.dialog({
        title: `${title} - 详情`,
        message,
        html: true,
        ok: {label: '关闭', color: 'primary', flat: true},
    });
}