import { Tag, Calendar, Github, Sparkles, Wrench, Zap } from 'lucide-react';
import changelogRaw from '../../../CHANGELOG.md?raw';
import { RELEASES_URL } from '../config/site';

interface ChangeItem {
  type: 'feature' | 'fix' | 'improvement' | 'breaking';
  text: string;
}

interface Version {
  version: string;
  date: string;
  type: 'major' | 'minor' | 'patch';
  changes: ChangeItem[];
}

const changeTypeConfig = {
  feature: { icon: Sparkles, gradient: 'from-emerald-500 to-teal-500', label: '新功能' },
  fix: { icon: Wrench, gradient: 'from-red-500 to-orange-500', label: '修复' },
  improvement: { icon: Zap, gradient: 'from-blue-500 to-cyan-500', label: '优化' },
  breaking: { icon: Tag, gradient: 'from-purple-500 to-pink-500', label: '重大变更' }
};

const versionTypeConfig = {
  major: { badge: 'Major', gradient: 'from-purple-500 to-pink-500' },
  minor: { badge: 'Minor', gradient: 'from-blue-500 to-cyan-500' },
  patch: { badge: 'Patch', gradient: 'from-gray-500 to-gray-600' }
};

function inferVersionType(version: string): Version['type'] {
  const parts = version.split('.').map((part) => Number.parseInt(part, 10) || 0);
  const minor = parts[1] ?? 0;
  const patch = parts[2] ?? 0;
  if (minor === 0 && patch === 0) return 'major';
  if (patch === 0) return 'minor';
  return 'patch';
}

function mapSectionType(section: string): ChangeItem['type'] {
  const key = section.trim().toLowerCase();
  if (key.startsWith('add')) return 'feature';
  if (key.startsWith('fix')) return 'fix';
  if (key.startsWith('improve')) return 'improvement';
  if (key.startsWith('break')) return 'breaking';
  return 'improvement';
}

function parseChangelog(markdown: string): Version[] {
  const blocks = markdown.split(/^##\s+/m).slice(1).map((block) => block.trim()).filter(Boolean);

  return blocks.map((block) => {
    const lines = block.split('\n');
    const header = lines.shift() ?? '';
    const match = header.match(/^\[(.+?)\](?:\s+-\s+(\d{4}-\d{2}-\d{2}))?/);
    const version = match?.[1] ?? header.trim();
    const date = match?.[2] ?? '日期待补充';

    const changes: ChangeItem[] = [];
    let currentType: ChangeItem['type'] = 'improvement';

    for (const rawLine of lines) {
      const line = rawLine.trim();
      if (!line) continue;
      const sectionMatch = line.match(/^###\s+(.+)$/);
      if (sectionMatch) {
        currentType = mapSectionType(sectionMatch[1]);
        continue;
      }
      if (/^[-*]\s+/.test(line)) {
        changes.push({
          type: currentType,
          text: line.replace(/^[-*]\s*/, '').trim()
        });
      }
    }

    return {
      version,
      date,
      type: inferVersionType(version),
      changes
    } satisfies Version;
  });
}

const versions = parseChangelog(changelogRaw);
const latest = versions[0];

export default function Changelog() {
  return (
    <div className="min-h-screen bg-gradient-to-b from-slate-950 via-slate-900 to-slate-950">
      <div className="fixed inset-0 overflow-hidden pointer-events-none">
        <div className="absolute top-1/4 left-1/3 w-96 h-96 bg-purple-500/10 rounded-full blur-3xl" />
        <div className="absolute bottom-1/3 right-1/3 w-80 h-80 bg-blue-500/10 rounded-full blur-3xl" />
      </div>

      <div className="relative z-10">
        <div className="py-20 sm:py-28 px-4 sm:px-6 lg:px-8">
          <div className="max-w-4xl mx-auto text-center">
            <div className="inline-flex items-center space-x-2 px-4 py-2 bg-white/5 backdrop-blur-sm border border-white/10 rounded-full mb-6">
              <Tag size={16} className="text-orange-400" />
              <span className="text-sm text-gray-300">版本历史</span>
            </div>
            <h1 className="text-4xl sm:text-5xl lg:text-6xl font-bold mb-6">
              <span className="bg-gradient-to-r from-white to-gray-400 bg-clip-text text-transparent">
                更新日志
              </span>
            </h1>
            <p className="text-xl text-gray-400 max-w-2xl mx-auto">
              官网直接读取仓库 CHANGELOG.md，保证版本信息与项目发布记录一致
            </p>
          </div>
        </div>

        {latest && (
          <section className="py-8 px-4 sm:px-6 lg:px-8">
            <div className="max-w-4xl mx-auto">
              <div className="relative p-8 bg-gradient-to-br from-orange-500/20 via-pink-500/20 to-rose-500/20 backdrop-blur-sm rounded-2xl border border-orange-500/30 overflow-hidden">
                <div className="absolute inset-0 bg-gradient-to-r from-orange-500/10 via-pink-500/10 to-rose-500/10 animate-gradient-shift" />
                <div className="relative">
                  <div className="flex items-center space-x-3 mb-4">
                    <div className="px-3 py-1 bg-gradient-to-r from-orange-500 to-pink-500 rounded-full">
                      <span className="text-white text-sm font-medium">最新版本</span>
                    </div>
                    <a
                      href={RELEASES_URL}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="flex items-center space-x-2 text-orange-400 hover:text-orange-300 transition-colors"
                    >
                      <Github size={18} />
                      <span className="text-sm">查看 Release</span>
                    </a>
                  </div>
                  <div className="text-4xl font-bold text-white mb-2">{latest.version}</div>
                  <div className="flex items-center space-x-2 text-gray-400">
                    <Calendar size={16} />
                    <span className="text-sm">{latest.date}</span>
                  </div>
                </div>
              </div>
            </div>
          </section>
        )}

        <section className="py-16 px-4 sm:px-6 lg:px-8">
          <div className="max-w-4xl mx-auto">
            <div className="relative">
              <div className="absolute left-8 top-0 bottom-0 w-px bg-gradient-to-b from-orange-500 via-pink-500 to-transparent" />
              <div className="space-y-8">
                {versions.map((release) => (
                  <div key={release.version} className="relative pl-20">
                    <div className={`absolute left-6 w-5 h-5 rounded-full bg-gradient-to-r ${versionTypeConfig[release.type].gradient} border-4 border-slate-900 shadow-lg`} />
                    <div className="group p-6 bg-white/5 backdrop-blur-sm rounded-xl border border-white/10 hover:border-white/20 transition-all">
                      <div className="flex flex-wrap items-center gap-4 mb-4">
                        <h3 className="text-2xl font-bold text-white">{release.version}</h3>
                        <div className={`px-2 py-0.5 bg-gradient-to-r ${versionTypeConfig[release.type].gradient} rounded-full`}>
                          <span className="text-white text-xs font-medium">{versionTypeConfig[release.type].badge}</span>
                        </div>
                        <span className="flex items-center text-gray-500 text-sm">
                          <Calendar size={14} className="mr-1" />
                          {release.date}
                        </span>
                      </div>

                      <div className="space-y-2">
                        {release.changes.map((change, idx) => {
                          const Icon = changeTypeConfig[change.type].icon;
                          return (
                            <div key={idx} className="flex items-start space-x-3">
                              <div className={`flex-shrink-0 w-8 h-8 rounded-lg bg-gradient-to-br ${changeTypeConfig[change.type].gradient} p-0.5`}>
                                <div className="w-full h-full rounded-lg bg-slate-900 flex items-center justify-center">
                                  <Icon className={`bg-gradient-to-br ${changeTypeConfig[change.type].gradient} bg-clip-text text-transparent`} size={16} />
                                </div>
                              </div>
                              <div className="flex-1 pt-1">
                                <span className="text-gray-300 text-sm">{change.text}</span>
                              </div>
                              <div className={`px-2 py-0.5 bg-gradient-to-r ${changeTypeConfig[change.type].gradient} rounded-full`}>
                                <span className="text-white text-xs">{changeTypeConfig[change.type].label}</span>
                              </div>
                            </div>
                          );
                        })}
                      </div>

                      <div className="absolute inset-0 bg-gradient-to-r from-orange-500/5 via-pink-500/5 to-rose-500/5 opacity-0 group-hover:opacity-100 rounded-xl transition-opacity pointer-events-none" />
                    </div>
                  </div>
                ))}
              </div>
            </div>
          </div>
        </section>

        <section className="py-16 px-4 sm:px-6 lg:px-8">
          <div className="max-w-4xl mx-auto">
            <div className="p-8 bg-white/5 backdrop-blur-sm rounded-2xl border border-white/10 text-center">
              <h3 className="text-xl font-bold text-white mb-3">查看完整历史</h3>
              <p className="text-gray-400 mb-6">访问 GitHub Releases 页面查看所有版本的详细变更</p>
              <a
                href={RELEASES_URL}
                target="_blank"
                rel="noopener noreferrer"
                className="inline-flex items-center space-x-2 px-6 py-3 bg-white/10 hover:bg-white/20 text-white font-medium rounded-xl border border-white/20 transition-all group"
              >
                <Github size={18} />
                <span>GitHub Releases</span>
              </a>
            </div>
          </div>
        </section>
      </div>

      <style>{`
        @keyframes gradient-shift {
          0%, 100% { transform: translateX(0) translateY(0); }
          25% { transform: translateX(-2%) translateY(2%); }
          50% { transform: translateX(2%) translateY(-2%); }
          75% { transform: translateX(-2%) translateY(-2%); }
        }
      `}</style>
    </div>
  );
}
