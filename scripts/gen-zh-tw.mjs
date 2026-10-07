// Generate src/i18n/locales/zh-TW.json from zh.json with OpenCC (s2twp) plus
// a small override map for UI terms where Taiwan usage differs from the
// phrase dictionary. Run: node scripts/gen-zh-tw.mjs
import { readFileSync, writeFileSync } from 'node:fs'
import * as OpenCC from 'opencc-js'

const converter = OpenCC.Converter({ from: 'cn', to: 'twp' })
const src = JSON.parse(readFileSync(new URL('../src/i18n/locales/zh.json', import.meta.url), 'utf8'))

// Taiwan UI conventions the phrase table does not cover (applied after conversion).
const OVERRIDES = [
  ['設置', '設定'],
  ['默認', '預設'],
  ['信息', '資訊'],
  ['字體', '字型'],
  ['鼠標', '滑鼠'],
  ['點擊', '點選'],
  ['窗口', '視窗'],
  ['快捷鍵', '快捷鍵'],
  ['菜單', '選單'],
  ['數據', '資料'],
  ['視頻', '影片'],
  ['音頻', '音訊'],
  ['軟件', '軟體'],
  ['硬件', '硬體'],
  ['網絡', '網路'],
  ['服務器', '伺服器'],
  ['用戶', '使用者'],
  ['帳戶', '帳號'],
  ['賬戶', '帳號'],
  ['登錄', '登入'],
  ['註銷', '登出'],
  ['退出登錄', '登出'],
  ['密碼', '密碼'],
  ['郵箱', '信箱'],
  ['發送', '傳送'],
  ['保存', '儲存'],
  ['加載', '載入'],
  ['刷新', '重新整理'],
  ['複製', '複製'],
  ['粘貼', '貼上'],
  ['剪貼板', '剪貼簿'],
  ['文件', '檔案'],
  ['文檔', '文件'],
  ['支持', '支援'],
  ['啟用', '啟用'],
  ['禁用', '停用'],
  ['創建', '建立'],
  ['刪除', '刪除'],
  ['搜索', '搜尋'],
  ['質量', '品質'],
  ['反饋', '回饋'],
  ['程序', '程式'],
  ['應用程序', '應用程式'],
  ['優化', '最佳化'],
  ['智能', '智慧'],
  ['屏幕', '螢幕'],
  ['打印', '列印'],
  ['調試', '偵錯'],
  ['操作系統', '作業系統'],
  ['快捷方式', '捷徑'],
  ['綁定', '綁定'],
  ['識別', '辨識'],
  ['語音識別', '語音辨識'],
  ['錄音', '錄音'],
  ['潤色', '潤飾'],
  ['轉錄', '轉錄'],
  ['雲端', '雲端'],
  ['訂閱', '訂閱'],
  ['升級', '升級'],
  ['詞典', '字典'],
  ['字典', '字典'],
  ['句號', '句號'],
  ['標點', '標點'],
  ['通過', '透過'],
  ['項目', '專案'],
  ['查看', '檢視'],
  ['重啟', '重新啟動'],
  ['服務', '服務'],
  ['高級', '進階'],
  ['隱藏', '隱藏'],
  ['膠囊', '膠囊'],
]

function convertValue(value) {
  if (typeof value === 'string') {
    let out = converter(value)
    for (const [from, to] of OVERRIDES) {
      if (from !== to) out = out.split(from).join(to)
    }
    return out
  }
  if (Array.isArray(value)) return value.map(convertValue)
  if (value && typeof value === 'object') {
    return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, convertValue(v)]))
  }
  return value
}

const out = convertValue(src)
writeFileSync(
  new URL('../src/i18n/locales/zh-TW.json', import.meta.url),
  JSON.stringify(out, null, 2) + '\n',
)
console.log('wrote src/i18n/locales/zh-TW.json')
