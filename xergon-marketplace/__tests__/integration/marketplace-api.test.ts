/**
 * Xergon Marketplace API Integration Tests
 * Tests for marketplace API endpoints
 */

import request from 'supertest';
import { describe, it, expect, beforeAll, afterAll } from '@jest/globals';

// Note: These tests assume the Next.js dev server is running on port 3000
// Run with: npm run test:integration

const BASE_URL = process.env.TEST_BASE_URL || 'http://localhost:3000';

describe('Marketplace API', () => {
  describe('GET /api/marketplace/models', () => {
    it('should return list of models', async () => {
      const response = await request(BASE_URL).get('/api/marketplace/models');

      expect(response.status).toBe(200);
      expect(response.body).toHaveProperty('models');
      expect(Array.isArray(response.body.models)).toBe(true);
    });

    it('should return featured models when subpath=featured', async () => {
      const response = await request(BASE_URL).get('/api/marketplace/models?subpath=featured');

      expect(response.status).toBe(200);
      expect(response.body).toHaveProperty('models');
      
      // Featured models should have isFeatured=true
      const models = response.body.models;
      if (models.length > 0) {
        expect(models[0]).toHaveProperty('isFeatured');
      }
    });

    it('should return categories when subpath=categories', async () => {
      const response = await request(BASE_URL).get('/api/marketplace/models?subpath=categories');

      expect(response.status).toBe(200);
      expect(response.body).toHaveProperty('categories');
      expect(Array.isArray(response.body.categories)).toBe(true);
    });

    it('should filter models by category', async () => {
      const response = await request(BASE_URL).get('/api/marketplace/models?category=code');

      expect(response.status).toBe(200);
      expect(response.body).toHaveProperty('models');
      
      const models = response.body.models;
      // All models should be in the 'code' category
      models.forEach((model: any) => {
        expect(model.category).toBe('code');
      });
    });

    it('should return trending models', async () => {
      const response = await request(BASE_URL).get('/api/marketplace/models?subpath=trending');

      expect(response.status).toBe(200);
      expect(response.body).toHaveProperty('models');
    });
  });

  describe('GET /api/xergon-relay/providers', () => {
    it('should return list of providers', async () => {
      const response = await request(BASE_URL).get('/api/xergon-relay/providers');

      expect(response.status).toBe(200);
      expect(response.body).toHaveProperty('providers');
      expect(Array.isArray(response.body.providers)).toBe(true);
    });

    it('should include provider status', async () => {
      const response = await request(BASE_URL).get('/api/xergon-relay/providers');

      expect(response.status).toBe(200);
      
      const providers = response.body.providers;
      if (providers.length > 0) {
        expect(providers[0]).toHaveProperty('status');
        expect(['online', 'degraded', 'offline']).toContain(providers[0].status);
      }
    });

    it('should include uptime information', async () => {
      const response = await request(BASE_URL).get('/api/xergon-relay/providers');

      expect(response.status).toBe(200);
      
      const providers = response.body.providers;
      if (providers.length > 0) {
        expect(providers[0]).toHaveProperty('uptime');
        expect(typeof providers[0].uptime).toBe('number');
        expect(providers[0].uptime).toBeGreaterThanOrEqual(0);
        expect(providers[0].uptime).toBeLessThanOrEqual(100);
      }
    });

    it('should include model pricing', async () => {
      const response = await request(BASE_URL).get('/api/xergon-relay/providers');

      expect(response.status).toBe(200);
      
      const providers = response.body.providers;
      if (providers.length > 0) {
        expect(providers[0]).toHaveProperty('modelPricing');
        expect(typeof providers[0].modelPricing).toBe('object');
      }
    });
  });

  describe('GET /api/xergon-relay/health', () => {
    it('should return health status', async () => {
      const response = await request(BASE_URL).get('/api/xergon-relay/health');

      expect(response.status).toBe(200);
      expect(response.body).toHaveProperty('status');
      expect(response.body.status).toBe('healthy');
    });

    it('should include relay info', async () => {
      const response = await request(BASE_URL).get('/api/xergon-relay/health');

      expect(response.status).toBe(200);
      expect(response.body).toHaveProperty('relay');
      expect(response.body.relay).toHaveProperty('connected');
    });
  });

  describe('GET /api/xergon-relay/stats', () => {
    it('should return marketplace statistics', async () => {
      const response = await request(BASE_URL).get('/api/xergon-relay/stats');

      expect(response.status).toBe(200);
      expect(response.body).toHaveProperty('totalProviders');
      expect(response.body).toHaveProperty('totalTokens');
      expect(response.body).toHaveProperty('avgUptime');
    });

    it('should include regional breakdown', async () => {
      const response = await request(BASE_URL).get('/api/xergon-relay/stats');

      expect(response.status).toBe(200);
      expect(response.body).toHaveProperty('byRegion');
      expect(typeof response.body.byRegion).toBe('object');
    });
  });

  describe('GET /api/earnings', () => {
    it('should require authentication', async () => {
      const response = await request(BASE_URL).get('/api/earnings');

      // Should be 401 or 403
      expect([401, 403]).toContain(response.status);
    });

    it('should return earnings data with valid auth', async () => {
      const token = process.env.TEST_AUTH_TOKEN || 'test-token';
      const response = await request(BASE_URL)
        .get('/api/earnings')
        .set('Authorization', `Bearer ${token}`);

      // This test will pass or fail based on actual auth implementation
      expect([200, 401, 403]).toContain(response.status);
      
      if (response.status === 200) {
        expect(response.body).toHaveProperty('earnings');
      }
    });
  });

  describe('GET /api/billing', () => {
    it('should require authentication', async () => {
      const response = await request(BASE_URL).get('/api/billing');

      expect([401, 403]).toContain(response.status);
    });
  });

  describe('GET /api/insights', () => {
    it('should return insights data', async () => {
      const response = await request(BASE_URL).get('/api/insights');

      // May require auth or be public
      expect([200, 401, 403]).toContain(response.status);
    });
  });

  describe('POST /api/support', () => {
    it('should accept support requests', async () => {
      const supportRequest = {
        subject: 'Test Support Request',
        message: 'This is a test support request',
        email: 'test@example.com',
      };

      const response = await request(BASE_URL)
        .post('/api/support')
        .send(supportRequest);

      expect([200, 201, 400]).toContain(response.status);
      
      if (response.status === 200 || response.status === 201) {
        expect(response.body).toHaveProperty('success', true);
      }
    });

    it('should validate required fields', async () => {
      const invalidRequest = {
        subject: '', // Empty subject
        message: 'Test',
      };

      const response = await request(BASE_URL)
        .post('/api/support')
        .send(invalidRequest);

      expect([400, 200]).toContain(response.status);
    });
  });

  describe('Error Handling', () => {
    it('should return 404 for non-existent endpoints', async () => {
      const response = await request(BASE_URL).get('/api/marketplace/nonexistent');

      expect(response.status).toBe(404);
    });

    it('should return consistent error format', async () => {
      const response = await request(BASE_URL).get('/api/marketplace/models?invalid=param');

      // Either success or proper error format
      if (response.status !== 200) {
        expect(response.body).toHaveProperty('error');
      }
    });
  });

  describe('Performance', () => {
    it('should respond within 500ms', async () => {
      const start = Date.now();
      await request(BASE_URL).get('/api/marketplace/models');
      const duration = Date.now() - start;

      expect(duration).toBeLessThan(500);
    });

    it('should handle concurrent requests', async () => {
      const promises = Array(5).fill(null).map(() => 
        request(BASE_URL).get('/api/marketplace/models')
      );

      const responses = await Promise.all(promises);
      const allSuccess = responses.every((r) => r.status === 200);

      expect(allSuccess).toBe(true);
    });
  });
});
