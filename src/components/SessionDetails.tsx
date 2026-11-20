import type { PointerEventHandler, Ref } from 'react';
import type { SessionGroup } from '../lib/useSessionDetails';
import type { SessionDetail, SessionKind } from '../lib/types';

const labels: Record<SessionKind, string> = { working: '工作中', completed: '未读' };

function projectLabel(session: SessionDetail) {
  const project = session.project?.trim();
  if (project) return project;
  const directory = session.workingDirectory;
  return directory ? `工作目录 · ${directory}` : '未归属';
}

function groupProjects(groups: SessionGroup[]) {
  const projects = new Map<string | null, { name: string; sessions: { item: SessionDetail; kind: SessionKind }[] }>();
  for (const { kind, sessions } of groups) {
    for (const item of sessions) {
      const key = item.projectGroupKey;
      let project = projects.get(key);
      if (!project) {
        project = { name: projectLabel(item), sessions: [] };
        projects.set(key, project);
      }
      project.sessions.push({ item, kind });
    }
  }
  return [...projects.entries()];
}

export function SessionDetails({ groups, visible, regionRef, scrollRef, onPointerEnter, onPointerLeave }: {
  groups: SessionGroup[];
  visible: boolean;
  regionRef: Ref<HTMLElement>;
  scrollRef: Ref<HTMLDivElement>;
  onPointerEnter: PointerEventHandler<HTMLElement>;
  onPointerLeave: PointerEventHandler<HTMLElement>;
}) {
  const anchoredToCompleted = groups.length === 1 && groups[0].kind === 'completed';
  const showRowStatus = groups.length > 1;
  return <aside ref={regionRef} role="region" aria-label="会话详情" aria-hidden={!visible} inert={!visible}
    className={`session-details${anchoredToCompleted ? ' session-details--completed' : ''}`}
    onPointerEnter={onPointerEnter} onPointerLeave={onPointerLeave}>
    <div ref={scrollRef} className="session-details-scroll">
      <div className="session-details-legend">
        {groups.map(({ kind }) => <span key={kind}><i aria-hidden="true" className={`session-detail-dot session-detail-dot--${kind}`} />{labels[kind]}</span>)}
      </div>
      {groupProjects(groups).map(([key, project]) => <section key={key ?? 'unassigned'} aria-label={project.name} className="session-details-project">
        <h2>{project.name}</h2>
        <ul>{project.sessions.map(({ item, kind }) => <li key={item.id}>
          {showRowStatus && <i role="img" aria-label={labels[kind]} className={`session-detail-dot session-detail-dot--${kind}`} />}
          <span className="session-detail-title">{item.title?.trim() || '未命名会话'}</span>
        </li>)}</ul>
      </section>)}
    </div>
  </aside>;
}
