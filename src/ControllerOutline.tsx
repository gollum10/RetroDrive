// Plain outline drawings of the 12 controller types (RDS-028). Original line art that
// shows a layout's shape only: no photos, logos or copied artwork.
type P = { x: number; y: number };

const Cross = ({ x, y, s = 9 }: P & { s?: number }) =>
  <path d={`M${x - s / 3} ${y - s} h${(2 * s) / 3} v${(2 * s) / 3} h${(2 * s) / 3} v${(2 * s) / 3} h${(-2 * s) / 3} v${(2 * s) / 3} h${(-2 * s) / 3} v${(-2 * s) / 3} h${(-2 * s) / 3} v${(-2 * s) / 3} h${(2 * s) / 3} Z`}/>;
const Diamond = ({ x, y, d = 9, r = 4 }: P & { d?: number; r?: number }) =>
  <>{[[0, -d], [d, 0], [0, d], [-d, 0]].map(([a, b], i) => <circle key={i} cx={x + a} cy={y + b} r={r}/>)}</>;
const Stick = ({ x, y }: P) => <><circle cx={x} cy={y} r={10}/><circle cx={x} cy={y} r={5}/></>;
// A gamepad with two grips (dual-analog and offset-stick types).
const PAD = 'M52 26 H148 C172 26 188 44 192 70 C196 98 186 114 170 112 C157 110 150 96 140 88 H60 C50 96 43 110 30 112 C14 114 4 98 8 70 C12 44 28 26 52 26 Z';

const drawings: Record<string, JSX.Element> = {
  CT01: <><rect x={60} y={40} width={80} height={70} rx={8}/><line x1={100} y1={75} x2={100} y2={28}/><circle cx={100} cy={20} r={9}/><circle cx={74} cy={52} r={6}/></>,
  CT02: <><rect x={70} y={6} width={60} height={108} rx={10}/><circle cx={100} cy={30} r={15}/>{[0, 1, 2, 3].map(r => [0, 1, 2].map(c => <rect key={`${r}${c}`} x={81 + c * 14} y={54 + r * 13} width={10} height={9} rx={2}/>))}<rect x={64} y={40} width={6} height={18} rx={2}/><rect x={130} y={40} width={6} height={18} rx={2}/></>,
  CT03: <><rect x={14} y={30} width={172} height={64} rx={6}/><Cross x={48} y={62} s={13}/><rect x={86} y={66} width={12} height={5} rx={2.5}/><rect x={104} y={66} width={12} height={5} rx={2.5}/><circle cx={140} cy={66} r={8}/><circle cx={162} cy={60} r={8}/></>,
  CT04: <><rect x={10} y={30} width={180} height={68} rx={34}/><path d="M28 32 Q44 20 66 26"/><path d="M172 32 Q156 20 134 26"/><Cross x={48} y={64} s={13}/><Diamond x={150} y={64} d={12} r={6}/><rect x={86} y={66} width={12} height={5} rx={2.5}/><rect x={104} y={66} width={12} height={5} rx={2.5}/></>,
  CT05: <><path d="M20 40 C20 22 60 18 100 18 C140 18 180 22 180 40 C180 60 172 108 158 110 C146 112 142 80 134 66 H118 C116 90 112 114 100 114 C88 114 84 90 82 66 H66 C58 80 54 112 42 110 C28 108 20 60 20 40 Z"/><Cross x={44} y={42} s={10}/><Stick x={100} y={60}/><circle cx={142} cy={50} r={5}/><circle cx={152} cy={40} r={5}/><Diamond x={162} y={30} d={7} r={3.2}/></>,
  CT06: <><path d={PAD}/><Cross x={50} y={52} s={11}/><Diamond x={150} y={52} d={11} r={5}/><Stick x={78} y={82}/><Stick x={122} y={82}/></>,
  CT07: <><path d={PAD}/><Stick x={46} y={52}/><Cross x={78} y={80} s={10}/><Diamond x={154} y={52} d={11} r={5}/><Stick x={122} y={82}/></>,
  CT08: <><rect x={84} y={6} width={32} height={108} rx={14}/><Cross x={100} y={28} s={8}/><circle cx={100} cy={52} r={6}/><circle cx={100} cy={86} r={3}/><circle cx={100} cy={96} r={3}/><path d="M128 40 Q140 52 128 64"/><path d="M138 32 Q156 52 138 72"/><path d="M148 24 Q172 52 148 80"/></>,
  CT09: <><path d="M10 60 H190 L180 104 H20 Z"/><line x1={50} y1={70} x2={50} y2={36}/><circle cx={50} cy={28} r={12}/>{[0, 1, 2, 3].map(i => <circle key={`a${i}`} cx={98 + i * 22} cy={70 - (i === 0 ? 0 : 4)} r={7}/>)}{[0, 1, 2, 3].map(i => <circle key={`b${i}`} cx={98 + i * 22} cy={90 - (i === 0 ? 0 : 4)} r={7}/>)}</>,
  CT10: <><rect x={14} y={44} width={172} height={64} rx={8}/><circle cx={70} cy={76} r={22}/><path d="M58 66 Q70 58 82 66"/><circle cx={136} cy={60} r={12}/><path d="M124 60 v-14 M148 60 v-14 M136 48 v-12"/><circle cx={130} cy={92} r={6}/><circle cx={154} cy={92} r={6}/></>,
  CT11: <><path d="M20 40 H170 V62 H96 L90 70 L98 110 H70 L62 70 L58 62 H20 Z"/><path d="M90 70 C92 84 80 88 76 78"/><line x1={170} y1={46} x2={190} y2={46}/><circle cx={186} cy={18} r={8}/><path d="M186 6 v6 M186 24 v6 M174 18 h6 M192 18 h6"/></>,
  CT12: <><circle cx={100} cy={62} r={50}/><circle cx={100} cy={62} r={38}/><circle cx={100} cy={70} r={12}/><path d="M62 62 H88 M112 62 H138 M100 82 V100"/></>,
};

export default function ControllerOutline({ typeId, size = 160, label }: { typeId: string; size?: number; label: string }) {
  const art = drawings[typeId];
  if (!art) return null;
  return <svg className="controller-outline" viewBox="0 0 200 120" width={size} height={(size * 120) / 200} role="img" aria-label={label}
    fill="none" stroke="currentColor" strokeWidth={2.5} strokeLinecap="round" strokeLinejoin="round">{art}</svg>;
}
