import { computed, ref } from 'vue'

export type Locale = 'zh' | 'en'

const STORAGE_KEY = 'kst.locale'

const messages = {
  zh: {
    brandSubtitle: 'Kaiyun Satellite Tool · 空间任务分析与仿真平台',
    appTitle: 'KST · Kaiyun Satellite Tool',
    phaseLabel: 'Phase 0 · 基础打通',
    logout: '退出登录',
    unknownUser: '未知用户',

    orbitParams: '轨道参数',
    epochUtc: '时刻（UTC）',
    semiMajorAxis: '半长轴 a（km）',
    eccentricity: '偏心率 e',
    inclination: '倾角 i（°）',
    raan: '升交点赤经 Ω（°）',
    argPerigee: '近地点幅角 ω（°）',
    trueAnomaly: '真近点角 ν（°）',
    compute: '计算并显示',
    computeNote: '时间转换、开普勒递推与 J2000→地固系旋转均由 Rust 后端完成，前端仅负责展示。',

    viewportTip: '左键旋转 · 滚轮缩放 · 右键/中键平移',
    resultTitle: '计算结果',
    utc: 'UTC',
    etSeconds: 'ET (s)',
    eciPosition: 'ECI 位置（km）',
    eciVelocity: 'ECI 速度（km/s）',
    ecefPosition: '地固系位置（km）',
    geodetic: '大地坐标',
    longitude: '经度',
    latitude: '纬度',
    altitude: '高度',
    noResult: '尚未计算',
    computeFailed: '计算失败',

    login: '登录',
    username: '用户名',
    password: '密码',
    usernamePlaceholder: '请输入用户名',
    passwordPlaceholder: '请输入密码',
    usernameRequired: '请输入用户名',
    passwordRequired: '请输入密码',
    loginFailed: '登录失败'
  },
  en: {
    brandSubtitle: 'Kaiyun Satellite Tool · Space Mission Analysis & Simulation Platform',
    appTitle: 'KST · Kaiyun Satellite Tool',
    phaseLabel: 'Phase 0 · Foundation',
    logout: 'Sign Out',
    unknownUser: 'Unknown user',

    orbitParams: 'Orbit Parameters',
    epochUtc: 'Epoch (UTC)',
    semiMajorAxis: 'Semi-major axis a (km)',
    eccentricity: 'Eccentricity e',
    inclination: 'Inclination i (deg)',
    raan: 'RAAN Ω (deg)',
    argPerigee: 'Argument of perigee ω (deg)',
    trueAnomaly: 'True anomaly ν (deg)',
    compute: 'Compute & Display',
    computeNote: 'Time conversion, Kepler propagation and J2000→Earth-fixed rotation are computed by the Rust backend; the frontend only renders.',

    viewportTip: 'Drag to rotate · Wheel to zoom · Right/middle drag to pan',
    resultTitle: 'Computed State',
    utc: 'UTC',
    etSeconds: 'ET (s)',
    eciPosition: 'ECI position (km)',
    eciVelocity: 'ECI velocity (km/s)',
    ecefPosition: 'Earth-fixed position (km)',
    geodetic: 'Geodetic coordinates',
    longitude: 'Longitude',
    latitude: 'Latitude',
    altitude: 'Altitude',
    noResult: 'Not computed yet',
    computeFailed: 'Computation failed',

    login: 'Sign In',
    username: 'Username',
    password: 'Password',
    usernamePlaceholder: 'Enter username',
    passwordPlaceholder: 'Enter password',
    usernameRequired: 'Username is required',
    passwordRequired: 'Password is required',
    loginFailed: 'Login failed'
  }
} as const

export type MessageKey = keyof typeof messages.zh

const locale = ref<Locale>(loadLocale())

function loadLocale(): Locale {
  const stored = localStorage.getItem(STORAGE_KEY)
  return stored === 'en' ? 'en' : 'zh'
}

export function useI18n() {
  const t = (key: MessageKey) => messages[locale.value][key]

  const switchLabel = computed(() => (locale.value === 'zh' ? 'EN' : '中文'))

  function toggleLocale() {
    locale.value = locale.value === 'zh' ? 'en' : 'zh'
    localStorage.setItem(STORAGE_KEY, locale.value)
  }

  return { locale, t, switchLabel, toggleLocale }
}
