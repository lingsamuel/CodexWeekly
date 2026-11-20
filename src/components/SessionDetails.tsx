import type { PointerEventHandler, Ref } from 'react';
import type { SessionGroup } from '../lib/useSessionDetails';
import type { SessionDetail } from '../lib/types';

function projectLabel(session: SessionDetail) {
  const project = session.project?.trim();
  if (project) return project;
  const directory = session.workingDirectory;
  return directory ? `工作目录 · ${directory}` : '未知项目';
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
  return <aside ref={regionRef} role="region" aria-label="会话详情" aria-hidden={!visible} inert={!visible}
    className={`session-details${anchoredToCompleted ? ' session-details--completed' : ''}`}
    onPointerEnter={onPointerEnter} onPointerLeave={onPointerLeave}>
    <div ref={scrollRef} className="session-details-scroll">
      {groups.map(({ kind, sessions }) => <section key={kind} className={`session-details-group session-details-group--${kind}`}>
        <h2>{kind === 'working' ? '工作中' : '未读完成'}</h2>
        <ul>{sessions.map(session => {
          const title = session.title?.trim() || '未命名会话';
          const project = projectLabel(session);
          return <li key={session.id}>
            <div className="session-detail-title" title={title}>{title}</div>
            <div className="session-detail-project" title={project}>{project}</div>
          </li>;
        })}</ul>
      </section>)}
    </div>
  </aside>;
}
