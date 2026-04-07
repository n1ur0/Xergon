/**
 * ModelSelector -- dropdown component for selecting AI models.
 *
 * Shows model list with descriptions, supports search/filter.
 */

import React, { useState, useRef, useEffect, useCallback } from 'react';
import type { Model } from '../types';

export interface ModelSelectorProps {
  models: Model[];
  selectedModel: string;
  onSelect: (model: string) => void;
  disabled?: boolean;
}

export function ModelSelector({
  models,
  selectedModel,
  onSelect,
  disabled = false,
}: ModelSelectorProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [search, setSearch] = useState('');
  const containerRef = useRef<HTMLDivElement>(null);

  // Close on outside click
  useEffect(() => {
    function handleClickOutside(e: MouseEvent) {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        setIsOpen(false);
      }
    }
    if (isOpen) {
      document.addEventListener('mousedown', handleClickOutside);
      return () => document.removeEventListener('mousedown', handleClickOutside);
    }
  }, [isOpen]);

  const filteredModels = models.filter(m =>
    m.id.toLowerCase().includes(search.toLowerCase()),
  );

  const handleSelect = useCallback((modelId: string) => {
    onSelect(modelId);
    setIsOpen(false);
    setSearch('');
  }, [onSelect]);

  return (
    <div className="xergon-model-selector" ref={containerRef}>
      <button
        className="xergon-model-selector-btn"
        onClick={() => setIsOpen(!isOpen)}
        disabled={disabled}
        title="Select model"
      >
        <span className="xergon-model-selector-label">
          {selectedModel || 'Select model'}
        </span>
        <span className="xergon-model-selector-arrow">
          {isOpen ? '▲' : '▼'}
        </span>
      </button>

      {isOpen && (
        <div className="xergon-model-selector-dropdown">
          <input
            className="xergon-model-selector-search"
            type="text"
            placeholder="Search models..."
            value={search}
            onChange={e => setSearch(e.target.value)}
            autoFocus
          />
          <ul className="xergon-model-selector-list">
            {filteredModels.length === 0 && (
              <li className="xergon-model-selector-empty">No models found</li>
            )}
            {filteredModels.map(model => (
              <li
                key={model.id}
                className={`xergon-model-selector-item ${model.id === selectedModel ? 'selected' : ''}`}
                onClick={() => handleSelect(model.id)}
              >
                <span className="xergon-model-selector-item-id">{model.id}</span>
                {model.pricing && (
                  <span className="xergon-model-selector-item-price">{model.pricing}</span>
                )}
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}

export default ModelSelector;
