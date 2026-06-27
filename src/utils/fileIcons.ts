// 文件图标映射（简化版，20 种常见类型）

/** 文件扩展名到 SVG 图标的映射 */
const ICON_MAP: Record<string, string> = {
  // TypeScript / JavaScript
  ts: '<svg width="16" height="16" viewBox="0 0 16 16" fill="none"><rect width="16" height="16" rx="2" fill="#3178C6"/><text x="8" y="12" font-size="10" fill="white" text-anchor="middle" font-weight="bold">TS</text></svg>',
  tsx: '<svg width="16" height="16" viewBox="0 0 16 16" fill="none"><rect width="16" height="16" rx="2" fill="#3178C6"/><text x="8" y="12" font-size="9" fill="white" text-anchor="middle" font-weight="bold">TSX</text></svg>',
  js: '<svg width="16" height="16" viewBox="0 0 16 16" fill="none"><rect width="16" height="16" rx="2" fill="#F7DF1E"/><text x="8" y="12" font-size="10" fill="black" text-anchor="middle" font-weight="bold">JS</text></svg>',
  jsx: '<svg width="16" height="16" viewBox="0 0 16 16" fill="none"><rect width="16" height="16" rx="2" fill="#61DAFB"/><text x="8" y="12" font-size="9" fill="black" text-anchor="middle" font-weight="bold">JSX</text></svg>',

  // Rust
  rs: '<svg width="16" height="16" viewBox="0 0 16 16" fill="none"><rect width="16" height="16" rx="2" fill="#CE422B"/><text x="8" y="12" font-size="10" fill="white" text-anchor="middle" font-weight="bold">RS</text></svg>',

  // Python
  py: '<svg width="16" height="16" viewBox="0 0 16 16" fill="none"><rect width="16" height="16" rx="2" fill="#3776AB"/><text x="8" y="12" font-size="10" fill="white" text-anchor="middle" font-weight="bold">PY</text></svg>',

  // Java
  java: '<svg width="16" height="16" viewBox="0 0 16 16" fill="none"><rect width="16" height="16" rx="2" fill="#007396"/><text x="8" y="12" font-size="8" fill="white" text-anchor="middle" font-weight="bold">JAVA</text></svg>',

  // 配置文件
  json: '<svg width="16" height="16" viewBox="0 0 16 16" fill="none"><rect width="16" height="16" rx="2" fill="#292929"/><text x="8" y="12" font-size="8" fill="#FFC107" text-anchor="middle" font-weight="bold">JSON</text></svg>',
  yaml: '<svg width="16" height="16" viewBox="0 0 16 16" fill="none"><rect width="16" height="16" rx="2" fill="#CB171E"/><text x="8" y="12" font-size="8" fill="white" text-anchor="middle" font-weight="bold">YAML</text></svg>',
  yml: '<svg width="16" height="16" viewBox="0 0 16 16" fill="none"><rect width="16" height="16" rx="2" fill="#CB171E"/><text x="8" y="12" font-size="9" fill="white" text-anchor="middle" font-weight="bold">YML</text></svg>',
  toml: '<svg width="16" height="16" viewBox="0 0 16 16" fill="none"><rect width="16" height="16" rx="2" fill="#9C4221"/><text x="8" y="12" font-size="8" fill="white" text-anchor="middle" font-weight="bold">TOML</text></svg>',

  // 文档
  md: '<svg width="16" height="16" viewBox="0 0 16 16" fill="none"><rect width="16" height="16" rx="2" fill="#083FA1"/><text x="8" y="12" font-size="10" fill="white" text-anchor="middle" font-weight="bold">MD</text></svg>',
  txt: '<svg width="16" height="16" viewBox="0 0 16 16" fill="none"><rect width="16" height="16" rx="2" fill="#6B7280"/><text x="8" y="12" font-size="9" fill="white" text-anchor="middle" font-weight="bold">TXT</text></svg>',

  // Web
  html: '<svg width="16" height="16" viewBox="0 0 16 16" fill="none"><rect width="16" height="16" rx="2" fill="#E34F26"/><text x="8" y="12" font-size="8" fill="white" text-anchor="middle" font-weight="bold">HTML</text></svg>',
  css: '<svg width="16" height="16" viewBox="0 0 16 16" fill="none"><rect width="16" height="16" rx="2" fill="#1572B6"/><text x="8" y="12" font-size="9" fill="white" text-anchor="middle" font-weight="bold">CSS</text></svg>',
  scss: '<svg width="16" height="16" viewBox="0 0 16 16" fill="none"><rect width="16" height="16" rx="2" fill="#CC6699"/><text x="8" y="12" font-size="8" fill="white" text-anchor="middle" font-weight="bold">SCSS</text></svg>',

  // 图片
  png: '<svg width="16" height="16" viewBox="0 0 16 16" fill="none"><rect width="16" height="16" rx="2" fill="#9333EA"/><text x="8" y="12" font-size="9" fill="white" text-anchor="middle" font-weight="bold">PNG</text></svg>',
  jpg: '<svg width="16" height="16" viewBox="0 0 16 16" fill="none"><rect width="16" height="16" rx="2" fill="#DC2626"/><text x="8" y="12" font-size="9" fill="white" text-anchor="middle" font-weight="bold">JPG</text></svg>',
  svg: '<svg width="16" height="16" viewBox="0 0 16 16" fill="none"><rect width="16" height="16" rx="2" fill="#FF9800"/><text x="8" y="12" font-size="9" fill="white" text-anchor="middle" font-weight="bold">SVG</text></svg>',

  // 默认
  default: '<svg width="16" height="16" viewBox="0 0 16 16" fill="none"><rect width="16" height="16" rx="2" fill="#9CA3AF"/><path d="M5 3h6l2 2v8H5V3z" fill="white"/></svg>',
};

/** 特殊文件名映射（优先级高于扩展名） */
const SPECIAL_FILES: Record<string, string> = {
  'package.json': '<svg width="16" height="16" viewBox="0 0 16 16" fill="none"><rect width="16" height="16" rx="2" fill="#CB3837"/><text x="8" y="12" font-size="9" fill="white" text-anchor="middle" font-weight="bold">NPM</text></svg>',
  'Cargo.toml': '<svg width="16" height="16" viewBox="0 0 16 16" fill="none"><rect width="16" height="16" rx="2" fill="#CE422B"/><circle cx="8" cy="8" r="4" fill="white"/></svg>',
  'tsconfig.json': '<svg width="16" height="16" viewBox="0 0 16 16" fill="none"><rect width="16" height="16" rx="2" fill="#3178C6"/><text x="8" y="12" font-size="8" fill="white" text-anchor="middle" font-weight="bold">TSCFG</text></svg>',
  '.gitignore': '<svg width="16" height="16" viewBox="0 0 16 16" fill="none"><rect width="16" height="16" rx="2" fill="#F05032"/><text x="8" y="12" font-size="9" fill="white" text-anchor="middle" font-weight="bold">GIT</text></svg>',
  'README.md': '<svg width="16" height="16" viewBox="0 0 16 16" fill="none"><rect width="16" height="16" rx="2" fill="#2563EB"/><text x="8" y="12" font-size="7" fill="white" text-anchor="middle" font-weight="bold">README</text></svg>',
};

/**
 * 获取文件图标 SVG
 * @param extension 文件扩展名（不含点号）
 * @param fileName 完整文件名（用于特殊文件检测）
 * @returns SVG 字符串
 */
export function getFileIcon(extension: string, fileName: string): string {
  // 优先检查特殊文件名
  if (SPECIAL_FILES[fileName]) {
    return SPECIAL_FILES[fileName];
  }

  // 匹配扩展名
  const lowerExt = extension.toLowerCase();
  return ICON_MAP[lowerExt] || ICON_MAP.default;
}

/**
 * 获取文件夹图标 SVG
 * @param folderName 文件夹名称
 * @returns SVG 字符串
 */
export function getFolderIcon(folderName: string): string {
  // 特殊文件夹图标
  const specialFolders: Record<string, string> = {
    src: '<svg width="16" height="16" viewBox="0 0 16 16" fill="none"><path d="M2 4l1-2h4l1 2h6v10H2V4z" fill="#60A5FA"/></svg>',
    node_modules: '<svg width="16" height="16" viewBox="0 0 16 16" fill="none"><path d="M2 4l1-2h4l1 2h6v10H2V4z" fill="#10B981"/></svg>',
    dist: '<svg width="16" height="16" viewBox="0 0 16 16" fill="none"><path d="M2 4l1-2h4l1 2h6v10H2V4z" fill="#F59E0B"/></svg>',
    '.git': '<svg width="16" height="16" viewBox="0 0 16 16" fill="none"><path d="M2 4l1-2h4l1 2h6v10H2V4z" fill="#F05032"/></svg>',
  };

  if (specialFolders[folderName]) {
    return specialFolders[folderName];
  }

  // 默认文件夹图标
  return '<svg width="16" height="16" viewBox="0 0 16 16" fill="none"><path d="M2 4l1-2h4l1 2h6v10H2V4z" fill="#94A3B8"/></svg>';
}
