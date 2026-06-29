import { useState, useEffect } from 'react';

export function useCockpitLayout() {
  const [leftOpen, setLeftOpen] = useState(() => {
    const saved = localStorage.getItem('helm-cockpit-left-open');
    return saved !== null ? saved === 'true' : true;
  });

  const [rightOpen, setRightOpen] = useState(() => {
    const saved = localStorage.getItem('helm-cockpit-right-open');
    return saved !== null ? saved === 'true' : true;
  });

  useEffect(() => {
    localStorage.setItem('helm-cockpit-left-open', String(leftOpen));
  }, [leftOpen]);

  useEffect(() => {
    localStorage.setItem('helm-cockpit-right-open', String(rightOpen));
  }, [rightOpen]);

  const toggleLeft = () => setLeftOpen((prev) => !prev);
  const toggleRight = () => setRightOpen((prev) => !prev);

  return {
    leftOpen,
    rightOpen,
    toggleLeft,
    toggleRight,
    setLeftOpen,
    setRightOpen,
  };
}
